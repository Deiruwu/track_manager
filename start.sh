#!/usr/bin/env bash
set -e

# ── Colores ───────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

ok()   { echo -e "${GREEN}[OK]${NC} $1"; }
err()  { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }
info() { echo -e "${YELLOW}[INFO]${NC} $1"; }

# ── Cargar .env ───────────────────────────────────────────────────────────────
if [ ! -f .env ]; then
    err "No se encontró .env — copia .env.example a .env y rellénalo."
fi
set -a; source .env; set +a

# ── Dependencias del sistema ──────────────────────────────────────────────────
info "Verificando dependencias del sistema..."

# cmd -> paquete de pacman
declare -A DEPS=(
    [psql]=postgresql
    [python]=python
    [ffmpeg]=ffmpeg
    [yt-dlp]=yt-dlp
    [cargo]=rust
)

MISSING=()
for cmd in "${!DEPS[@]}"; do
    command -v "$cmd" &>/dev/null || MISSING+=("${DEPS[$cmd]}")
done

if [ ${#MISSING[@]} -ne 0 ]; then
    info "Instalando: ${MISSING[*]}"
    sudo pacman -S --noconfirm --needed "${MISSING[@]}"
fi
ok "Dependencias listas."

# ── Base de datos ─────────────────────────────────────────────────────────────
info "Verificando base de datos..."

DB_EXISTS=$(psql -U "$POSTGRES_USER" -h "${POSTGRES_HOST%%:*}" -tAc \
    "SELECT 1 FROM pg_database WHERE datname='$POSTGRES_DB'" 2>/dev/null || true)

if [ "$DB_EXISTS" != "1" ]; then
    info "Creando base de datos '$POSTGRES_DB'..."
    createdb -U "$POSTGRES_USER" -h "${POSTGRES_HOST%%:*}" "$POSTGRES_DB"
    ok "Base de datos creada."
else
    ok "Base de datos '$POSTGRES_DB' ya existe."
fi

# ── Python venv ───────────────────────────────────────────────────────────────
info "Verificando entorno Python..."

if [ ! -d "Music_Services/.venv" ]; then
    info "Creando .venv..."
    python -m venv Music_Services/.venv
fi

info "Instalando dependencias Python..."
Music_Services/.venv/bin/pip install --upgrade pip -q
Music_Services/.venv/bin/pip install -r Music_Services/requirements.txt -q
ok "Entorno Python listo."

# ── Compilar Rust ─────────────────────────────────────────────────────────────
info "Compilando..."
SQLX_OFFLINE=true cargo build --release
ok "Binario listo en target/release/track_manager"