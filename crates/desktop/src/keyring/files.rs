//! Bounded file reads and atomic, private-mode file publication.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpgme::Context;
use kryptopass_secure_memory::SensitiveBytes;
use uuid::Uuid;

pub(super) const MAX_INPUT_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub(super) const MAX_KEY_BLOB_BYTES: usize = 16 * 1024 * 1024;

pub(super) struct PublishOutcome {
    pub durability_warning: bool,
}

/// A uniquely reserved, mode-0600 temporary file whose open handle is kept
/// through the write and publication.
pub(super) struct SecureTempFile {
    path: PathBuf,
    file: Option<File>,
}

pub(super) struct SecureTempDirectory(PathBuf);

impl SecureTempDirectory {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for SecureTempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl SecureTempFile {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_mut(&mut self) -> Result<&mut File, String> {
        self.file
            .as_mut()
            .ok_or_else(|| "El archivo temporal ya está cerrado.".to_owned())
    }
}

impl Drop for SecureTempFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn create_secure_temp_directory(
    parent: PathBuf,
    prefix: &str,
) -> Result<SecureTempDirectory, String> {
    for _ in 0..8 {
        let path = parent.join(format!(".{prefix}-{}", Uuid::new_v4()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&path) {
            Ok(()) => return Ok(SecureTempDirectory(path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(crate::i18n::format(
                    "No se pudo crear una carpeta temporal privada: %1",
                    &[error.to_string()],
                ));
            }
        }
    }
    Err("No se pudo reservar una carpeta temporal privada.".into())
}

pub(super) fn create_secure_temp_file(
    parent: PathBuf,
    prefix: &str,
) -> Result<SecureTempFile, String> {
    for _ in 0..8 {
        let path = parent.join(format!(".{prefix}-{}.tmp", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).read(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => {
                return Ok(SecureTempFile {
                    path,
                    file: Some(file),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(crate::i18n::format(
                    "No se pudo crear un temporal seguro: %1",
                    &[error.to_string()],
                ));
            }
        }
    }
    Err("No se pudo reservar un nombre temporal único.".into())
}

pub(super) fn read_bounded_file(
    path: &Path,
    limit: u64,
    description: &str,
) -> Result<SensitiveBytes, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path).map_err(|error| {
        crate::i18n::format(
            "No se pudo abrir %1: %2",
            &[format!("{}", path.display()), error.to_string()],
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        crate::i18n::format(
            "No se pudo consultar %1: %2",
            &[format!("{}", path.display()), error.to_string()],
        )
    })?;
    if !metadata.is_file() {
        return Err(crate::i18n::format(
            "%1 debe ser un archivo local regular.",
            &[description.to_string()],
        ));
    }
    if metadata.len() > limit {
        return Err(crate::i18n::format(
            "%1 supera el tamaño permitido.",
            &[description.to_string()],
        ));
    }
    let capacity = usize::try_from(metadata.len()).unwrap_or(0);
    let mut bytes = SensitiveBytes::with_capacity(capacity);
    std::io::copy(
        &mut Read::by_ref(&mut file).take(limit.saturating_add(1)),
        &mut bytes,
    )
    .map_err(|error| {
        crate::i18n::format(
            "No se pudo leer %1: %2",
            &[format!("{}", path.display()), error.to_string()],
        )
    })?;
    if bytes.len() as u64 > limit {
        return Err(crate::i18n::format(
            "%1 supera el tamaño permitido.",
            &[description.to_string()],
        ));
    }
    Ok(bytes)
}

pub(super) fn write_atomic_new(
    path: &Path,
    bytes: &[u8],
    context: &Context,
) -> Result<PublishOutcome, String> {
    if bytes.len() > MAX_KEY_BLOB_BYTES {
        return Err("La exportación supera el tamaño máximo permitido.".into());
    }
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "La ruta de salida debe señalar un archivo.".to_owned())?;
    let requested_parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(requested_parent).map_err(|error| {
        crate::i18n::format(
            "No se pudo abrir la carpeta de destino %1: %2",
            &[format!("{}", requested_parent.display()), error.to_string()],
        )
    })?;
    if !parent.is_dir() {
        return Err("La carpeta de destino no existe o no es una carpeta.".into());
    }
    reject_protected_destination(&parent, file_name, context)?;
    let destination = parent.join(file_name);
    if destination.exists() {
        return Err(crate::i18n::format(
            "El archivo %1 ya existe; la exportación no lo sobrescribirá.",
            &[format!("{}", destination.display())],
        ));
    }

    let mut temporary = create_secure_temp_file(parent.clone(), "kryptopass-export")?;
    temporary.file_mut()?.write_all(bytes).map_err(|error| {
        crate::i18n::format(
            "No se pudo escribir la exportación: %1",
            &[error.to_string()],
        )
    })?;
    temporary.file_mut()?.sync_all().map_err(|error| {
        crate::i18n::format(
            "No se pudo sincronizar la exportación: %1",
            &[error.to_string()],
        )
    })?;
    fs::hard_link(temporary.path(), &destination).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            crate::i18n::format(
                "El archivo %1 ya existe; no se sobrescribió.",
                &[format!("{}", destination.display())],
            )
        } else {
            crate::i18n::format(
                "No se pudo publicar la exportación de forma atómica: %1",
                &[error.to_string()],
            )
        }
    })?;

    // The output is already visible at `destination`. A directory-sync failure
    // affects crash durability only and must not turn publication into an error
    // that could prompt the caller to repeat the export.
    let durability_warning = File::open(&parent)
        .and_then(|directory| directory.sync_all())
        .is_err();
    Ok(PublishOutcome { durability_warning })
}

fn reject_protected_destination(
    parent: &Path,
    filename: &std::ffi::OsStr,
    context: &Context,
) -> Result<(), String> {
    if Path::new(filename)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "db" | "db3" | "sqlite" | "sqlite3"
            )
        })
        .unwrap_or(false)
    {
        return Err(
            "No se permite usar una extensión de base de datos como destino de exportación.".into(),
        );
    }
    if let Ok(home) = context.engine_info().home_dir() {
        if let Ok(canonical_home) = fs::canonicalize(home) {
            if parent.starts_with(canonical_home) {
                return Err("Guarda la exportación fuera de la carpeta privada de GnuPG.".into());
            }
        }
    }
    Ok(())
}

pub(super) struct BoundedBufferWriter {
    storage: Arc<Mutex<SensitiveBytes>>,
    limit: usize,
}

impl BoundedBufferWriter {
    pub fn new(storage: Arc<Mutex<SensitiveBytes>>, limit: usize) -> Self {
        Self { storage, limit }
    }
}

impl Write for BoundedBufferWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buffer = self
            .storage
            .lock()
            .map_err(|_| io::Error::other("buffer de exportación bloqueado"))?;
        if bytes.len() > self.limit.saturating_sub(buffer.len()) {
            return Err(io::Error::other(
                "la exportación supera el tamaño permitido",
            ));
        }
        buffer.try_extend_from_slice(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
