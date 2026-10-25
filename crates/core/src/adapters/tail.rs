//! Lectura incremental de archivos JSONL que crecen (transcripts, ADR 0002 §5): desde el último
//! offset, con la línea incompleta guardada aparte y detección de truncado.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Tope por lectura: si el archivo creció más de esto entre dos escaneos, se salta al final.
const MAX_READ: u64 = 1024 * 1024;

pub struct Tail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    /// Se empezó a mitad de archivo: la primera línea está cortada y se descarta.
    skip_first: bool,
}

impl Tail {
    /// Abre el archivo dispuesto a leer solo sus últimos `back` bytes.
    pub fn open_at_end(path: &Path, back: u64) -> std::io::Result<Self> {
        let len = std::fs::metadata(path)?.len();
        let offset = len.saturating_sub(back);
        Ok(Self {
            path: path.to_owned(),
            offset,
            partial: Vec::new(),
            skip_first: offset > 0,
        })
    }

    /// Líneas completas añadidas desde la última lectura, sin el salto de línea.
    pub fn read_new(&mut self) -> std::io::Result<Vec<Vec<u8>>> {
        let mut file = std::fs::File::open(&self.path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            // Truncado o reescrito: se vuelve a empezar desde el principio.
            self.offset = 0;
            self.partial.clear();
            self.skip_first = false;
        }
        if len - self.offset > MAX_READ {
            self.offset = len - MAX_READ;
            self.partial.clear();
            self.skip_first = true;
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut chunk = Vec::with_capacity((len - self.offset) as usize);
        file.take(len - self.offset).read_to_end(&mut chunk)?;
        self.offset += chunk.len() as u64;
        self.partial.extend_from_slice(&chunk);
        let Some(last_nl) = self.partial.iter().rposition(|b| *b == b'\n') else {
            return Ok(Vec::new());
        };
        let rest = self.partial.split_off(last_nl + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        let mut lines: Vec<Vec<u8>> = complete
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(<[u8]>::to_vec)
            .collect();
        if self.skip_first && !lines.is_empty() {
            lines.remove(0);
            self.skip_first = false;
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kiti-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        let _ = std::fs::remove_file(&p);
        p
    }

    fn append(path: &std::path::Path, text: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    fn lines(t: &mut Tail) -> Vec<String> {
        t.read_new()
            .unwrap()
            .into_iter()
            .map(|l| String::from_utf8(l).unwrap())
            .collect()
    }

    #[test]
    fn solo_devuelve_lo_nuevo_y_completo() {
        let p = tmp("a.jsonl");
        append(&p, "uno\ndos\n");
        let mut t = Tail::open_at_end(&p, 1024).unwrap();
        assert_eq!(lines(&mut t), vec!["uno", "dos"]);
        assert!(lines(&mut t).is_empty());
        append(&p, "tres\ncua");
        assert_eq!(lines(&mut t), vec!["tres"]);
        append(&p, "tro\n");
        assert_eq!(lines(&mut t), vec!["cuatro"]);
    }

    #[test]
    fn al_abrir_por_el_final_descarta_la_primera_linea_cortada() {
        let p = tmp("b.jsonl");
        append(&p, "una linea larga\ncorta\n");
        // Últimos 8 bytes: "a\ncorta\n" → la línea cortada "a" no se devuelve.
        let mut t = Tail::open_at_end(&p, 8).unwrap();
        assert_eq!(lines(&mut t), vec!["corta"]);
    }

    #[test]
    fn si_el_archivo_se_trunca_vuelve_a_empezar() {
        let p = tmp("c.jsonl");
        append(&p, "viejo\nviejo2\n");
        let mut t = Tail::open_at_end(&p, 1024).unwrap();
        lines(&mut t);
        std::fs::write(&p, "nuevo\n").unwrap();
        assert_eq!(lines(&mut t), vec!["nuevo"]);
    }

    #[test]
    fn un_archivo_que_desaparece_es_un_error_y_no_un_panico() {
        let p = tmp("d.jsonl");
        append(&p, "x\n");
        let mut t = Tail::open_at_end(&p, 1024).unwrap();
        std::fs::remove_file(&p).unwrap();
        assert!(t.read_new().is_err());
    }
}
