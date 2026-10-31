//! Immutable, thread-scoped image copies made when `display_image` succeeds.
//!
//! The source path is deliberately not used by the web preview route. The
//! attachment lives beside its owning session and can only be read through a
//! validated thread ID plus a content hash.
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 32_000_000;
const MAX_THREAD_ATTACHMENTS: usize = 128;
const MAX_THREAD_ATTACHMENT_BYTES: u64 = 64 * 1024 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn set_private_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn private_permissions(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o077 == 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        true
    }
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn valid_thread_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() < 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_attachment_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn thread_id(data_root: &Path, session_path: &Path) -> Result<(PathBuf, String), String> {
    let root = data_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let metadata = fs::symlink_metadata(session_path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Image attachment session is not a regular file".into());
    }
    let session = session_path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if session.parent() != Some(root.as_path()) {
        return Err("Image attachment session is outside thread storage".into());
    }
    let name = session
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".jsonl"))
        .ok_or("Image attachment session path is invalid")?;
    if !valid_thread_id(name) {
        return Err("Image attachment session ID is invalid".into());
    }
    Ok((root, name.to_owned()))
}

fn attachment_dir(root: &Path, id: &str, create: bool) -> Result<PathBuf, String> {
    let path = root.join(format!("{id}.attachments"));
    if create {
        match fs::create_dir(&path) {
            Ok(()) => set_private_directory(&path).map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        sync_directory(root).map_err(|error| error.to_string())?;
    }
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Thread image attachment store is not a private directory".into());
    }
    if !private_permissions(&metadata) {
        return Err("Thread image attachment store permissions are too broad".into());
    }
    let canonical = path.canonicalize().map_err(|error| error.to_string())?;
    if canonical.parent() != Some(root) {
        return Err("Thread image attachment store escaped thread storage".into());
    }
    Ok(canonical)
}

/// Serialize capacity checks and insertion across concurrent tool workers.
fn lock_store(dir: &Path) -> Result<File, String> {
    use fs2::FileExt;

    let path = dir.join(".lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    let file = options.open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || !private_permissions(&metadata) {
        return Err("Thread image attachment lock is not private".into());
    }
    file.lock_exclusive().map_err(|error| error.to_string())?;
    Ok(file)
}

fn hash(bytes: &[u8]) -> String {
    crate::hashing::sha256(bytes)
}

fn image_type_and_dimensions(bytes: &[u8]) -> Result<(&'static str, u32, u32), String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Shared image exceeds 8 MiB".into());
    }
    let media_type = crate::polish::image_media_type(bytes)
        .ok_or("Shared attachment is not a supported image")?;
    let format = image::guess_format(bytes).map_err(|_| "Shared attachment image is invalid")?;
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| "Shared attachment image dimensions are invalid")?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err("Shared image exceeds 32 million pixels or has empty dimensions".into());
    }
    Ok((media_type, width, height))
}

fn open_read_nofollow(path: &Path) -> Result<File, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Shared image attachment is not a regular file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    options.open(path).map_err(|error| error.to_string())
}

fn verify_existing(path: &Path, attachment_id: &str, bytes: &[u8]) -> Result<(), String> {
    let file = open_read_nofollow(path)?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || !private_permissions(&metadata)
        || metadata.len() != bytes.len() as u64
    {
        return Err("Existing shared image attachment has invalid metadata".into());
    }
    let mut existing = Vec::with_capacity(bytes.len());
    file.take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut existing)
        .map_err(|error| error.to_string())?;
    if existing != bytes || hash(&existing) != attachment_id {
        return Err("Existing shared image attachment failed its content check".into());
    }
    Ok(())
}

fn store_capacity(dir: &Path, incoming: usize) -> Result<(), String> {
    let mut count = 0usize;
    let mut total = 0u64;
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        if name == ".lock" {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("Thread image attachment store contains an unsafe entry".into());
        }
        count = count.saturating_add(1);
        total = total.saturating_add(metadata.len());
    }
    if count >= MAX_THREAD_ATTACHMENTS
        || total.saturating_add(incoming as u64) > MAX_THREAD_ATTACHMENT_BYTES
    {
        return Err("Thread image attachment storage limit reached".into());
    }
    Ok(())
}

fn remove_interrupted_temps(dir: &Path) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_name().to_string_lossy().starts_with(".tmp-") {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("Thread image attachment store contains an unsafe entry".into());
        }
        fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Persist a successful display-only image under the owning session.
pub(crate) fn store(data_root: &Path, session_path: &Path, bytes: &[u8]) -> Result<String, String> {
    image_type_and_dimensions(bytes)?;
    let (root, id) = thread_id(data_root, session_path)?;
    let dir = attachment_dir(&root, &id, true)?;
    let _lock = lock_store(&dir)?;
    remove_interrupted_temps(&dir)?;
    let attachment_id = hash(bytes);
    let target = dir.join(&attachment_id);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err("Existing shared image attachment is not a regular file".into());
        }
        Ok(_) => {
            verify_existing(&target, &attachment_id, bytes)?;
            return Ok(attachment_id);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    store_capacity(&dir, bytes.len())?;

    let temporary = loop {
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(".tmp-{}-{sequence}", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
        match options.open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(&path);
                    return Err(error.to_string());
                }
                break path;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    };
    match fs::hard_link(&temporary, &target) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if let Err(error) = verify_existing(&target, &attachment_id, bytes) {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error.to_string());
        }
    }
    fs::remove_file(&temporary).map_err(|error| error.to_string())?;
    sync_directory(&dir).map_err(|error| error.to_string())?;
    Ok(attachment_id)
}

/// Read one immutable image only after resolving a thread-owned attachment ID.
pub(crate) fn read(
    data_root: &Path,
    session_path: &Path,
    attachment_id: &str,
) -> Result<(&'static str, Vec<u8>), String> {
    if !valid_attachment_id(attachment_id) {
        return Err("Invalid image attachment ID".into());
    }
    let (root, id) = thread_id(data_root, session_path)?;
    let dir = attachment_dir(&root, &id, false)?;
    let path = dir.join(attachment_id);
    let file = open_read_nofollow(&path)?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || !private_permissions(&metadata)
        || metadata.len() > MAX_IMAGE_BYTES as u64
    {
        return Err("Image attachment metadata is invalid".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if hash(&bytes) != attachment_id {
        return Err("Image attachment content check failed".into());
    }
    let (media_type, _, _) = image_type_and_dimensions(&bytes)?;
    Ok((media_type, bytes))
}

/// Resolve an attachment by the authenticated thread ID used by Connect.
pub(crate) fn read_thread(
    data_root: &Path,
    thread: &str,
    attachment_id: &str,
) -> Result<(&'static str, Vec<u8>), String> {
    if !valid_thread_id(thread) {
        return Err("Invalid thread ID".into());
    }
    let root = data_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let session_path = root.join(format!("{thread}.jsonl"));
    let metadata = fs::symlink_metadata(&session_path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Thread record is not a regular file".into());
    }
    read(&root, &session_path, attachment_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgba8(2, 3)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "gc-shared-image-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let session = root.join("thread_test.jsonl");
        fs::write(&session, b"{}").unwrap();
        (root, session)
    }

    #[test]
    fn attachment_is_content_addressed_thread_scoped_and_immutable() {
        let (root, session) = fixture();
        let bytes = png();
        let id = store(&root, &session, &bytes).unwrap();
        assert!(valid_attachment_id(&id));
        let dir = root.join("thread_test.attachments");
        fs::write(dir.join(".tmp-interrupted"), b"partial").unwrap();
        #[cfg(unix)]
        {
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(dir.join(&id)).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(store(&root, &session, &bytes).unwrap(), id);
        assert!(!dir.join(".tmp-interrupted").exists());
        let (media_type, loaded) = read(&root, &session, &id).unwrap();
        assert_eq!(media_type, "image/png");
        assert_eq!(loaded, bytes);
        assert!(read(&root, &session, "../../secret").is_err());
        assert!(read(&root, &session, &"0".repeat(64)).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn attachment_store_rejects_symlink_directory_and_symlink_blob() {
        let (root, session) = fixture();
        let bytes = png();
        let id = hash(&bytes);
        #[cfg(unix)]
        {
            let linked_session = root.join("thread_alias.jsonl");
            std::os::unix::fs::symlink(&session, &linked_session).unwrap();
            assert!(store(&root, &linked_session, &bytes).is_err());
            fs::remove_file(linked_session).unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("elsewhere"), root.join("thread_test.attachments"))
            .unwrap();
        assert!(store(&root, &session, &bytes).is_err());
        fs::remove_file(root.join("thread_test.attachments")).unwrap();
        fs::create_dir(root.join("thread_test.attachments")).unwrap();
        fs::set_permissions(
            root.join("thread_test.attachments"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            root.join("elsewhere"),
            root.join("thread_test.attachments").join(&id),
        )
        .unwrap();
        assert!(read(&root, &session, &id).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn thread_attachment_lookup_cannot_cross_threads_or_follow_session_links() {
        let (root, first_session) = fixture();
        let other_session = root.join("thread_other.jsonl");
        fs::write(&other_session, b"{}").unwrap();
        let bytes = png();
        let id = store(&root, &first_session, &bytes).unwrap();
        assert_eq!(read_thread(&root, "thread_test", &id).unwrap().1, bytes);
        assert!(read_thread(&root, "thread_other", &id).is_err());
        #[cfg(unix)]
        {
            let linked_session = root.join("thread_linked.jsonl");
            std::os::unix::fs::symlink(&first_session, &linked_session).unwrap();
            assert!(read_thread(&root, "thread_linked", &id).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}
