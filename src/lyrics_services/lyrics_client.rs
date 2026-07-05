use crate::lyrics_services::lyrics::LrcLibResponse;
use strsim::jaro_winkler;

const BASE_URL: &str = "https://lrclib.net/api";
const USER_AGENT: &str = "lyra_track_manager/1.0 (+https://github.com/dei)";
const MAX_DURATION_DELTA_SECONDS: f64 = 2.0;

#[derive(Debug)]
pub enum LyricsError {
    Network(String),
    NotFound,
}

impl std::fmt::Display for LyricsError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LyricsError::Network(e) => write!(f, "Error de red hacia LRCLIB: {e}"),
            LyricsError::NotFound => write!(f, "LRCLIB no encontró letras"),
        }
    }
}

impl std::error::Error for LyricsError {}

#[derive(Clone)]
pub struct LyricsClient {
    http: reqwest::Client,
}

impl LyricsClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("no se pudo construir el cliente HTTP de lyrics");

        Self { http }
    }

    async fn get_by_metadata(
        &self,
        track_name: &str,
        artist_name: &str,
        duration_seconds: i32,
    ) -> Result<LrcLibResponse, LyricsError> {
        let query = [
            ("track_name", track_name.to_string()),
            ("artist_name", artist_name.to_string()),
            ("duration", duration_seconds.to_string()),
        ];

        let resp = self
            .http
            .get(format!("{BASE_URL}/get"))
            .query(&query)
            .send()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(LyricsError::NotFound);
        }

        resp.error_for_status()
            .map_err(|e| LyricsError::Network(e.to_string()))?
            .json::<LrcLibResponse>()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))
    }

    /// Reemplazo de tu antiguo `search`. Devuelve todos los resultados crudos
    /// para que el validador estricto los filtre y puntúe.
    async fn search_fuzzy(&self, query: &[(&str, &str)]) -> Result<Vec<LrcLibResponse>, LyricsError> {
        let resp = self
            .http
            .get(format!("{BASE_URL}/search"))
            .query(query)
            .send()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| LyricsError::Network(e.to_string()))?;

        let results = resp
            .json::<Vec<LrcLibResponse>>()
            .await
            .map_err(|e| LyricsError::Network(e.to_string()))?;

        if results.is_empty() {
            return Err(LyricsError::NotFound);
        }

        Ok(results)
    }

    /// Estrategia de búsqueda exhaustiva priorizando letras sincronizadas.
    /// Si encuentra una letra plana válida, no se detiene: la guarda como fallback
    /// y continúa buscando la versión sincronizada en los endpoints más difusos.
    pub async fn find_best_lyrics(
        &self,
        track_name: &str,
        primary_artist: &str,
        duration_seconds: i32,
    ) -> Result<LrcLibResponse, LyricsError> {
        let mut fallback_plain: Option<LrcLibResponse> = None;

        // Closure helper: evalúa el mejor candidato válido de un endpoint.
        // Cortocircuita (retorna Some) SOLO si la letra es sincronizada.
        // Si es plana, la guarda en fallback y retorna None para que el pipeline siga.
        let mut process_candidate = |cand_opt: Option<LrcLibResponse>| -> Option<LrcLibResponse> {
            if let Some(cand) = cand_opt {
                let has_synced = cand.synced_lyrics.as_deref().is_some_and(|s| !s.trim().is_empty());
                if has_synced {
                    return Some(cand);
                }

                let has_plain = cand.plain_lyrics.as_deref().is_some_and(|s| !s.trim().is_empty());
                if fallback_plain.is_none() && has_plain {
                    fallback_plain = Some(cand);
                }
            }
            None
        };

        // 1. /get exacto
        if let Ok(r) = self.get_by_metadata(track_name, primary_artist, duration_seconds).await {
            if let Some(synced) = process_candidate(Some(r)) {
                return Ok(synced);
            }
        }

        let mut title_candidates = vec![track_name.to_string()];

        let cleaned = clean_title(track_name);
        if cleaned != track_name && !title_candidates.contains(&cleaned) {
            title_candidates.push(cleaned);
        }

        if let Some(cut) = cut_at_metadata_separator(track_name) {
            if !title_candidates.contains(&cut) {
                title_candidates.push(cut);
            }
        }

        // 2. /search por título limpio (Tolerancia a discrepancias en feats/tags de artista)
        for title in &title_candidates {
            let query = [("track_name", title.as_str())];
            if let Ok(results) = self.search_fuzzy(&query).await {
                let best_match = select_valid_match(results, track_name, duration_seconds);
                if let Some(synced) = process_candidate(best_match) {
                    return Ok(synced);
                }
            }
        }

        // 3. /search masivo con q="..." (Para cuando track y artista están revueltos en LRCLIB)
        for title in &title_candidates {
            let q_str = format!("{title} {primary_artist}");
            let query = [("q", q_str.as_str())];
            if let Ok(results) = self.search_fuzzy(&query).await {
                let best_match = select_valid_match(results, track_name, duration_seconds);
                if let Some(synced) = process_candidate(best_match) {
                    return Ok(synced);
                }
            }
        }

        // 4. Rescate por Artista + Duración + Script Guard (Romaji vs Kanji)
        if primary_artist.len() >= 3 && primary_artist.to_lowercase() != "desconocido" {
            let query = [("q", primary_artist)];
            if let Ok(results) = self.search_fuzzy(&query).await {
                let best_match = select_by_duration_and_script(results, track_name, duration_seconds);
                if let Some(synced) = process_candidate(best_match) {
                    return Ok(synced);
                }
            }
        }

        // Si recorrimos todo el catálogo de variantes y no hubo sincronizada,
        // devolvemos la plana que hayamos guardado en el camino, si existe.
        fallback_plain.ok_or(LyricsError::NotFound)
    }
}

// =====================================================================
// UTILS & VALIDATORS (Strict Zero False Positives)
// =====================================================================

fn clean_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let markers = ["feat.", "feat ", "ft.", "ft ", "with ", "remix", "live", "acoustic"];
    let mut cut_at = None;

    for (open, close) in [('(', ')'), ('[', ']')] {
        let mut search_from = 0;
        while let Some(rel_open) = lower[search_from..].find(open) {
            let abs_open = search_from + rel_open;
            let Some(rel_close) = lower[abs_open..].find(close) else { break };
            let abs_close = abs_open + rel_close;

            let inner = &lower[abs_open + 1..abs_close];
            if markers.iter().any(|m| inner.contains(m)) {
                cut_at = Some(cut_at.map_or(abs_open, |c: usize| c.min(abs_open)));
            }
            search_from = abs_close + 1;
        }
    }

    match cut_at {
        Some(idx) => title[..idx].trim_end().to_string(),
        None => title.trim().to_string(),
    }
}

fn cut_at_metadata_separator(title: &str) -> Option<String> {
    let cut_idx = title.find('/').into_iter().chain(title.find('×')).min()?;
    let cut = title[..cut_idx].trim();
    if cut.is_empty() { None } else { Some(cut.to_string()) }
}

fn has_non_latin_script(s: &str) -> bool {
    s.chars().any(|c| (c as u32) >= 0x0370)
}

fn select_valid_match(
    results: Vec<LrcLibResponse>,
    expected_title: &str,
    expected_duration_seconds: i32,
) -> Option<LrcLibResponse> {
    let expected_title_lower = expected_title.to_lowercase();

    let mut valid_candidates: Vec<(LrcLibResponse, f64)> = results
        .into_iter()
        .filter_map(|cand| {
            let duration = cand.duration?;
            let delta = (duration - expected_duration_seconds as f64).abs();
            if delta > MAX_DURATION_DELTA_SECONDS {
                return None;
            }

            let cand_title = cand.track_name.as_deref().unwrap_or("").to_lowercase();
            let similarity = jaro_winkler(&expected_title_lower, &cand_title);

            if similarity < 0.85 {
                return None;
            }

            Some((cand, delta))
        })
        .collect();

    // Prioriza letras sincronizadas por sobre las planas dentro de los validos,
    // y luego desempata por el que tenga el delta de tiempo más cercano a cero.
    valid_candidates.sort_by(|a, b| {
        let a_synced = a.0.synced_lyrics.is_some();
        let b_synced = b.0.synced_lyrics.is_some();
        b_synced.cmp(&a_synced).then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    valid_candidates.into_iter().map(|(cand, _)| cand).next()
}

fn select_by_duration_and_script(
    results: Vec<LrcLibResponse>,
    expected_title: &str,
    expected_duration_seconds: i32,
) -> Option<LrcLibResponse> {
    let expected_is_non_latin = has_non_latin_script(expected_title);

    let mut valid: Vec<(LrcLibResponse, f64)> = results
        .into_iter()
        .filter_map(|cand| {
            let duration = cand.duration?;
            let delta = (duration - expected_duration_seconds as f64).abs();
            if delta > MAX_DURATION_DELTA_SECONDS {
                return None;
            }

            let cand_title = cand.track_name.as_deref().unwrap_or("");
            let cand_is_non_latin = has_non_latin_script(cand_title);

            if expected_is_non_latin == cand_is_non_latin {
                return None;
            }

            Some((cand, delta))
        })
        .collect();

    valid.sort_by(|a, b| {
        let a_synced = a.0.synced_lyrics.is_some();
        let b_synced = b.0.synced_lyrics.is_some();
        b_synced.cmp(&a_synced).then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });

    valid.into_iter().map(|(cand, _)| cand).next()
}

impl Default for LyricsClient {
    fn default() -> Self {
        Self::new()
    }
}