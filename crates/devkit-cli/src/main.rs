use clap::{Parser, Subcommand};
use devkit_core::DevKit;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "devkit", about = "Entorno local portable (Nginx + PHP + MariaDB)", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Descarga binarios oficiales (HTTPS + SHA-256)
    Setup {
        /// Redis, Mailpit y mkcert. Node va en el setup básico.
        #[arg(long)]
        full: bool,
        /// Major de PHP (8.3 o 8.4)
        #[arg(long)]
        php: Option<String>,
    },
    /// Arranca el stack (detach por defecto)
    Start {
        service: Option<String>,
        #[arg(long)]
        foreground: bool,
    },
    Stop {
        service: Option<String>,
    },
    Status,
    /// Muestra qué proceso usa cada puerto del stack
    Ports,
    Reload,
    /// Cambia o lista versiones de PHP
    Php {
        #[command(subcommand)]
        cmd: PhpCmd,
    },
    /// Crea un proyecto en www/
    Create {
        kind: String,
        name: String,
    },
    /// Certificados locales (mkcert) y HTTPS :8443
    Ssl,
    /// Zip portable
    Pack {
        out: Option<PathBuf>,
    },
    /// Terminal con PATH de DevKit
    Terminal,
    /// Credenciales locales de MariaDB
    Db {
        #[command(subcommand)]
        cmd: DbCmd,
    },
    /// Leer logs del kit / proyectos (php_error, nginx, Laravel…)
    Logs {
        #[command(subcommand)]
        cmd: LogsCmd,
    },
    #[command(hide = true)]
    DnsServe,
}

#[derive(Subcommand)]
enum PhpCmd {
    List,
    Use { version: String },
}

#[derive(Subcommand)]
enum DbCmd {
    /// Muestra usuario y si hay contraseña (sin revelarla)
    Status,
    /// Alinea a root sin contraseña (default local) y aplica si MariaDB está arriba
    Sync,
}

#[derive(Subcommand)]
enum LogsCmd {
    /// Lista logs del kit y, por defecto, hints de frameworks en www/
    List {
        #[arg(long)]
        kit_only: bool,
    },
    /// Últimas líneas (`php`, `nginx-error`, o ruta bajo www/)
    Tail {
        id: String,
        #[arg(long, default_value_t = 80)]
        lines: usize,
    },
    /// Busca texto en un log
    Search {
        id: String,
        query: String,
        #[arg(long, default_value_t = 50)]
        max: usize,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> devkit_core::Result<()> {
    let cli = Cli::parse();
    let mut kit = DevKit::load()?;
    match cli.cmd {
        Commands::Setup { full, php } => {
            for line in kit.setup(full, php.as_deref())? {
                println!("{line}");
            }
        }
        Commands::Start {
            service,
            foreground,
        } => {
            let detach = !foreground;
            if let Some(id) = service {
                let snap = kit.start_service(&id, detach)?;
                println!(
                    "{} {} :{} ({})",
                    snap.display_name,
                    snap.status.as_str(),
                    snap.port,
                    snap.detail
                );
            } else {
                let st = kit.start_all(detach)?;
                print_status(&st);
                println!("abre {}", kit.default_url());
                if foreground {
                    println!("Ctrl+C para detener…");
                    wait_ctrl_c();
                    let _ = kit.stop_all();
                }
            }
        }
        Commands::Stop { service } => {
            if let Some(id) = service {
                let snap = kit.stop_service(&id)?;
                println!("{} {}", snap.display_name, snap.status.as_str());
            } else {
                print_status(&kit.stop_all()?);
            }
        }
        Commands::Status => print_status(&kit.status()?),
        Commands::Ports => {
            let checks = kit.check_ports()?;
            for c in &checks {
                let mark = if c.is_conflict() {
                    "CONFLICTO"
                } else if matches!(c.owner, devkit_core::PortOwner::Ours { .. }) {
                    "DEVKIT"
                } else {
                    "libre"
                };
                println!("  [{mark:9}] {}", c.summary());
                if c.is_conflict() {
                    println!("            → {}", c.hint);
                }
            }
            if checks.iter().any(|c| c.is_conflict()) {
                eprintln!("tip: apaga Laragon/XAMPP o cambia puertos en etc/devkit/config.toml");
            }
        }
        Commands::Reload => {
            let sites = kit.reload_sites()?;
            println!("{} sitios", sites.len());
            for s in sites {
                println!("  {} -> {}", s.hostname, s.url);
            }
        }
        Commands::Php { cmd } => match cmd {
            PhpCmd::List => {
                println!("activo: {}", kit.config.php_version);
                for v in kit.php_versions()? {
                    println!("{v}");
                }
            }
            PhpCmd::Use { version } => {
                kit.use_php(&version)?;
                println!("PHP {version} activo");
            }
        },
        Commands::Create { kind, name } => {
            let p = kit.create_site(&kind, &name)?;
            println!("{}", p.display());
        }
        Commands::Ssl => println!("{}", kit.enable_ssl()?),
        Commands::Pack { out } => println!("{}", kit.pack(out)?.display()),
        Commands::Terminal => kit.open_terminal()?,
        Commands::Db { cmd } => match cmd {
            DbCmd::Status => {
                let c = kit.db_credentials()?;
                let pwd = if c.password.is_empty() {
                    "vacía"
                } else {
                    "definida"
                };
                println!("MariaDB local: usuario={} contraseña={}", c.user, pwd);
            }
            DbCmd::Sync => println!("{}", kit.align_mysql_local_default()?),
        },
        Commands::Logs { cmd } => match cmd {
            LogsCmd::List { kit_only } => {
                for e in kit.list_logs(!kit_only)? {
                    let mark = if e.exists { "ok" } else { "-" };
                    println!(
                        "[{mark}] {:<40} {:>8} B  {}",
                        e.id,
                        e.size_bytes,
                        e.path.display()
                    );
                }
            }
            LogsCmd::Tail { id, lines } => {
                let text = kit.tail_log(&id, Some(lines))?;
                if text.is_empty() {
                    println!("(vacío o inexistente: {id})");
                } else {
                    println!("{text}");
                }
            }
            LogsCmd::Search { id, query, max } => {
                let hits = kit.search_log(&id, &query, Some(max))?;
                if hits.is_empty() {
                    println!("sin coincidencias");
                } else {
                    for h in hits {
                        println!("{h}");
                    }
                }
            }
        },
        Commands::DnsServe => {
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let handle = devkit_core::dns::spawn(stop.clone())?;
            println!("DNS *.test -> 127.0.0.1");
            wait_ctrl_c();
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = handle.join();
        }
    }
    Ok(())
}

fn print_status(st: &devkit_core::StackStatus) {
    println!("root {}", st.root.display());
    println!("php  {}", st.php_version);
    println!("dns  {}", st.dns);
    for s in &st.services {
        println!(
            "  [{:7}] {:<16} {:>5}  {}",
            s.status.as_str(),
            s.display_name,
            s.port,
            s.detail
        );
    }
    for site in &st.sites {
        println!("  site {} {}", site.hostname, site.url);
    }
    for c in &st.port_checks {
        if c.is_conflict() {
            eprintln!("puerto: {}", c.summary());
        }
    }
    for w in &st.warnings {
        eprintln!("aviso: {w}");
    }
}

fn wait_ctrl_c() {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = ctrlc::set_handler(move || {
        let _ = tx.send(());
    });
    let _ = rx.recv();
}
