//! Analyze a Proton Pass JSON export without changing it.
use kp_login_analysis::{Analyzer, Export, domain_folder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("Indica la ruta al JSON exportado de Proton Pass")?;
    let show_folders = args.any(|arg| arg == "--domain-folders");
    let export = Export::from_path(path)?;
    let analysis = Analyzer::embedded()?.analyze(&export);
    println!(
        "{} ámbitos, {} usuarios, {} credenciales, {} entradas sin asignar",
        analysis.scopes.len(),
        analysis.user_count(),
        analysis.credential_count(),
        analysis.unassigned.len()
    );
    for group in &analysis.scopes {
        println!("{}", group.scope);
        for user in &group.users {
            println!("  {}: {}", user.user, user.decision.label());
        }
    }
    if show_folders {
        let mut rows: Vec<_> = export
            .iter_logins()
            .map(|login| {
                let folder = login
                    .invalid_urls
                    .is_empty()
                    .then(|| domain_folder(login.urls.iter().map(|url| url.as_str())))
                    .flatten()
                    .unwrap_or_else(|| "sin dominio".to_owned());
                (folder, login.title)
            })
            .collect();
        rows.sort();
        for (folder, title) in rows {
            println!("  {folder} / {title}");
        }
    }
    Ok(())
}
