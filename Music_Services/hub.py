import asyncio
import json
import logging
from ytmusicapi import YTMusic

from repositories.yt.search import YTMusicSearchRepository
from repositories.yt.track import YTMusicTrackRepository
from repositories.yt.radio import YTMusicRadioRepository
from repositories.yt.album import YTMusicAlbumRepository

# Importamos el nuevo servicio analizador
from services.analyzer import AnalysisService

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%H:%M:%S"
)


class MusicHubServer:
    def __init__(self, host: str = '127.0.0.1', port: int = 9999):
        self.host = host
        self.port = port

        logging.info("Inicializando cliente de YTMusic y repositorios...")
        self.yt_client = YTMusic()
        self.search_repo = YTMusicSearchRepository(self.yt_client)
        self.track_repo  = YTMusicTrackRepository(self.yt_client)
        self.radio_repo  = YTMusicRadioRepository(self.yt_client)
        self.album_repo  = YTMusicAlbumRepository(self.yt_client)

        # Instanciamos el servicio de análisis
        self.analyzer_service = AnalysisService()

        self.routes = {
            "search": self._handle_search,
            "track":  self._handle_track,
            "album":  self._handle_album,
            "radio":  self._handle_radio,
            "analyze_local_file": self._handle_analyze,
        }
        logging.info("Repositorios y rutas listos.")

    # ─────────────────────────────────────────
    # HANDLERS
    # ─────────────────────────────────────────

    async def _handle_analyze(self, payload: dict) -> dict:
        """
        Delega el procesamiento pesado de audio (FFT, ffprobe) a un thread
        para evitar bloquear el loop principal de asyncio.
        """
        file_path = payload.get("query")
        if not file_path:
            return {"status": "error", "message": "Falta la ruta del archivo en la query"}

        logging.info(f"Iniciando análisis en thread paralelo para: {file_path}")

        try:
            # Ejecución no bloqueante
            result = await asyncio.to_thread(self.analyzer_service.analyze, file_path)
            logging.info(f"Análisis completado para {file_path}: {result}")
            return {"status": "ok", "data": result}

        except FileNotFoundError as e:
            return {"status": "error", "message": str(e)}
        except Exception as e:
            logging.error(f"Fallo crítico en analizador para {file_path}: {e}")
            return {"status": "error", "message": "Fallo interno en el analizador de audio"}

    async def _handle_search(self, payload: dict) -> dict:
        query  = payload["query"]
        filter = payload.get("filter", "songs")   # songs | videos
        limit  = int(payload.get("limit", 5))

        if filter not in ("songs", "videos"):
            return {"status": "error", "message": f"Filtro inválido: {filter}. Usa 'songs' o 'videos'"}

        tracks = await self.search_repo.search(query=query, type=filter, limit=limit)
        return {"status": "ok", "data": [t.to_dict() for t in tracks]}

    async def _handle_track(self, payload: dict) -> dict:
        track = await self.track_repo.get_track(track_id=payload["query"])
        if track:
            return {"status": "ok", "data": track.to_dict()}
        return {"status": "error", "message": "Track no encontrado"}

    async def _handle_album(self, payload: dict) -> dict:
        tracks = await self.album_repo.get_album_tracks(album_id=payload["query"])
        return {"status": "ok", "data": [t.to_dict() for t in tracks]}

    async def _handle_radio(self, payload: dict) -> dict:
        limit  = int(payload.get("limit", 25))
        tracks = await self.radio_repo.get_radio_tracks(seed_track_id=payload["query"], limit=limit)
        return {"status": "ok", "data": [t.to_dict() for t in tracks]}

    async def process_command(self, raw_json: str) -> dict:
        try:
            payload = json.loads(raw_json)
            action  = payload.get("action")
            query   = payload.get("query", "").strip()

            if not action or not query:
                return {"status": "error", "message": "Faltan campos 'action' o 'query'"}

            handler = self.routes.get(action)
            if not handler:
                return {"status": "error", "message": f"Accion desconocida: {action}"}

            return await handler(payload)

        except json.JSONDecodeError:
            return {"status": "error", "message": "JSON invalido"}
        except Exception as e:
            logging.error(f"Error procesando comando: {e}")
            return {"status": "error", "message": "Error interno del servidor de metadatos"}

    async def handle_client(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        addr = writer.get_extra_info('peername')
        logging.info(f"Nueva conexion desde {addr}")
        try:
            while True:
                data = await reader.readline()
                if not data:
                    break
                message = data.decode('utf-8').strip()
                if not message:
                    continue
                logging.info(f"Recibido: {message}")
                response = (json.dumps(await self.process_command(message), ensure_ascii=False) + "\n").encode('utf-8')
                writer.write(response)
                await writer.drain()
        except asyncio.CancelledError:
            pass
        except Exception as e:
            logging.error(f"Error en la conexion con {addr}: {e}")
        finally:
            logging.info(f"Conexion cerrada con {addr}")
            writer.close()
            await writer.wait_closed()

    async def start(self):
        server = await asyncio.start_server(self.handle_client, self.host, self.port)
        logging.info(f"Music Hub escuchando en {server.sockets[0].getsockname()}")
        print(f"Music Hub escuchando en {server.sockets[0].getsockname()}", flush=True)
        async with server:
            await server.serve_forever()


if __name__ == "__main__":
    hub = MusicHubServer(host='127.0.0.1', port=9999)
    try:
        asyncio.run(hub.start())
    except KeyboardInterrupt:
        logging.info("Servidor apagado.")