import os
import json
import subprocess
import numpy as np
import librosa
import warnings
from typing import Optional

warnings.filterwarnings('ignore')

class AnalysisService:
    def __init__(self):
        self.camelot_map = self._build_camelot_map()

    def _build_camelot_map(self):
        return {
            'B Maj': '1B', 'F# Maj': '2B', 'C# Maj': '3B', 'G# Maj': '4B',
            'D# Maj': '5B', 'A# Maj': '6B', 'F Maj': '7B', 'C Maj': '8B',
            'G Maj': '9B', 'D Maj': '10B', 'A Maj': '11B', 'E Maj': '12B',
            'G# Min': '1A', 'D# Min': '2A', 'A# Min': '3A', 'F Min': '4A',
            'C Min': '5A', 'G Min': '6A', 'D Min': '7A', 'A Min': '8A',
            'E Min': '9A', 'B Min': '10A', 'F# Min': '11A', 'C# Min': '12A'
        }

    def analyze(self, file_path: str) -> dict:
        if not os.path.exists(file_path):
            raise FileNotFoundError(f"Archivo no encontrado en el servidor Python: {file_path}")

        bpm = self._get_bpm(file_path)
        key = self._get_key(file_path)
        sample_rate = self._get_sample_rate(file_path)

        return {
            "bpm": bpm,
            "camelotKey": key,
            "sample_rate_hz": sample_rate
        }

    def _get_sample_rate(self, file_path: str) -> Optional[int]:
        try:
            cmd = ["ffprobe", "-v", "error", "-select_streams", "a:0",
                   "-show_entries", "stream=sample_rate", "-of", "json", file_path]
            result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            if result.returncode != 0: return None
            data = json.loads(result.stdout)
            val = int(data.get("streams", [{}])[0].get("sample_rate", 0))
            return val if val > 0 else None
        except Exception:
            return None

    def _get_bpm(self, file_path: str) -> Optional[int]:
        try:
            y, sr = librosa.load(file_path, mono=True, offset=30, duration=60)
            tempo, _ = librosa.beat.beat_track(y=y, sr=sr)
            tempo_val = tempo[0] if isinstance(tempo, np.ndarray) else tempo
            return int(round(float(tempo_val)))
        except Exception:
            return None

    def _get_key(self, file_path: str) -> Optional[str]:
        try:
            y, sr = librosa.load(file_path, offset=30, duration=60)
            y_harmonic = librosa.effects.hpss(y)[0]
            chroma = librosa.feature.chroma_cqt(y=y_harmonic, sr=sr, fmin=librosa.note_to_hz('C2'), n_octaves=4)
            chroma_mean = np.mean(chroma, axis=1)

            major_profile = np.array([5.0, 2.0, 3.5, 2.0, 4.5, 4.0, 2.0, 4.5, 2.0, 3.5, 1.5, 4.0])
            minor_profile = np.array([5.0, 2.0, 3.5, 4.5, 2.0, 4.0, 2.0, 4.5, 3.5, 2.0, 1.5, 4.0])
            notes = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']

            scores = {f"{i}_Maj": np.corrcoef(chroma_mean, np.roll(major_profile, i))[0, 1] for i in range(12)}
            scores.update({f"{i}_Min": np.corrcoef(chroma_mean, np.roll(minor_profile, i))[0, 1] for i in range(12)})

            best_key_code = max(scores, key=scores.get)
            best_idx, best_mode = best_key_code.split('_')
            return self._to_camelot(f"{notes[int(best_idx)]} {best_mode}")
        except Exception:
            return None

    def _to_camelot(self, key_str: str) -> str:
        replacements = {"Db": "C#", "Eb": "D#", "Gb": "F#", "Ab": "G#", "Bb": "A#"}
        for old, new in replacements.items():
            key_str = key_str.replace(old, new)
        return self.camelot_map.get(key_str, "?")