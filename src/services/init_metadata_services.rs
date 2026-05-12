use std::path::PathBuf;
use std::env;
use std::error::Error;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::process::Stdio;

#[derive(Debug)]
pub struct PythonMicroservice {
    venv_path:   PathBuf,
    script_path: PathBuf,
}

impl PythonMicroservice {
    pub fn new(venv_relative: &str, script_relative: &str) -> Self {
        let root = env::current_dir().unwrap_or_default();
        Self {
            venv_path:   root.join(venv_relative),
            script_path: root.join(script_relative),
        }
    }

    /// Instala dependencias del requirements.txt dentro del venv
    async fn install_requirements(&self) -> Result<(), Box<dyn Error>> {
        let pip_exe = self.venv_path.join("bin").join("pip");
        let requirements = self.script_path
            .parent()
            .unwrap_or(&self.script_path)
            .join("requirements.txt");

        if !requirements.exists() {
            println!("[PIP] No se encontró requirements.txt, saltando.");
            return Ok(());
        }

        println!("[PIP] Instalando dependencias...");

        let status = Command::new(&pip_exe)
            .args(["install", "-r"])
            .arg(&requirements)
            .status()
            .await?;

        if !status.success() {
            return Err(format!("pip install falló con código: {}", status).into());
        }

        println!("[PIP] Dependencias instaladas.");
        Ok(())
    }

    /// Inicia el proceso de Python dentro del venv
    pub async fn spawn_service(&self) -> Result<(), Box<dyn Error>> {
        self.install_requirements().await?;

        let python_exe = self.venv_path.join("bin").join("python");
        println!("Ejecutándose desde: {:#?}", self);

        let mut std_cmd = Command::new(&python_exe);
        std_cmd.arg(&self.script_path)
            .env("VIRTUAL_ENV", &self.venv_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        unsafe {
            std_cmd.pre_exec(|| {
                let result = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                if result != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let mut child = Command::from(std_cmd).spawn()?;

        let stdout = child.stdout.take().ok_or("No se pudo capturar stdout")?;
        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next_line().await? {
            println!("[PYTHON] {line}");
            if line.contains("Music Hub escuchando") {
                println!("[Python] Microservicio listo.");
                break;
            }
        }

        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        Ok(())
    }
}
