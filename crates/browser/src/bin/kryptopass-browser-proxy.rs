//! Native messaging entry point. Standard output is reserved for browser frames.

fn main() {
    #[cfg(unix)]
    let result = (|| -> std::io::Result<()> {
        let mut arguments = std::env::args_os().skip(1);
        if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--socket")) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "expected --socket PATH",
            ));
        }
        let path = arguments.next().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "socket path is missing")
        })?;
        kryptopass_browser::run_proxy(std::path::Path::new(&path))
    })();
    #[cfg(not(unix))]
    let result: std::io::Result<()> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "browser integration currently requires Unix sockets",
    ));
    if let Err(error) = result {
        eprintln!("Browser proxy: {error}");
        std::process::exit(1);
    }
}
