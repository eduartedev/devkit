use crate::bind::assert_config_localhost_only;
use crate::error::Result;
use crate::model::{Site, LOCALHOST};
use crate::paths::{AppConfig, Paths};
use crate::site::{nginx_escape_root, nginx_escape_server_name};
use std::fs;
use std::path::Path;

pub fn render_main(paths: &Paths, cfg: &AppConfig, sites: &[Site]) -> Result<String> {
    let root = paths.posix(&paths.root);
    let logs = paths.posix(&paths.logs);
    let tmp = paths.posix(&paths.tmp);
    let www_default = paths.posix(&paths.www.join("default"));
    let mime = paths.posix(&paths.etc.join("nginx").join("mime.types"));
    let sites_dir = paths.posix(&paths.nginx_sites());
    let php = format!("{}:{}", cfg.bind, cfg.php_port);

    let ssl_block = if cfg.ssl && paths.ssl_cert().exists() {
        format!(
            r#"
    server {{
        listen {bind}:{https} ssl;
        server_name localhost 127.0.0.1 default.{tld};
        ssl_certificate {cert};
        ssl_certificate_key {key};
        root {www_default};
        include {sites_dir}/_ssl_common.conf;
        {php_loc}
    }}
"#,
            bind = cfg.bind,
            https = cfg.https_port,
            tld = cfg.tld,
            cert = paths.posix(&paths.ssl_cert()),
            key = paths.posix(&paths.ssl_key()),
            www_default = www_default,
            sites_dir = sites_dir,
            php_loc = php_location(&php),
        )
    } else {
        String::new()
    };

    let conf = format!(
        r#"worker_processes 1;
error_log {logs}/nginx-error.log warn;
pid {logs}/nginx.pid;

events {{
    worker_connections 128;
}}

http {{
    include {mime};
    default_type application/octet-stream;
    access_log {logs}/nginx-access.log;
    sendfile off;
    keepalive_timeout 65;
    client_max_body_size 64m;
    client_body_temp_path {tmp}/nginx_body;
    proxy_temp_path {tmp}/nginx_proxy;
    fastcgi_temp_path {tmp}/nginx_fastcgi;
    uwsgi_temp_path {tmp}/nginx_uwsgi;
    scgi_temp_path {tmp}/nginx_scgi;

    server {{
        listen {bind}:{http} default_server;
        server_name localhost 127.0.0.1 default.{tld};
        root {www_default};
        index index.php index.html;
        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}
        {php_loc}
    }}
    include {sites_dir}/*.conf;
    {ssl_block}
}}
"#,
        bind = cfg.bind,
        http = cfg.http_port,
        tld = cfg.tld,
        php_loc = php_location(&php),
        ssl_block = ssl_block,
    );

    let _ = (root, sites);
    assert_config_localhost_only(&conf)?;
    if cfg.bind != LOCALHOST {
        crate::bind::require_localhost(&cfg.bind)?;
    }
    Ok(conf)
}

fn php_location(fastcgi: &str) -> String {
    format!(
        r#"location ~ \.php$ {{
            try_files $uri =404;
            fastcgi_pass {fastcgi};
            fastcgi_index index.php;
            fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
            include fastcgi_params;
        }}"#
    )
}

pub fn render_site(cfg: &AppConfig, site: &Site, ssl: bool) -> Result<String> {
    let name = nginx_escape_server_name(&site.name)?;
    let hostname = format!("{name}.{}", cfg.tld);
    if hostname.contains('"') || hostname.contains(';') {
        return Err(crate::error::Error::msg("hostname inseguro"));
    }
    let root = nginx_escape_root(&site.root)?;
    let php = format!("{}:{}", cfg.bind, cfg.php_port);
    let listen = if ssl {
        format!(
            "listen {bind}:{https} ssl;\n        ssl_certificate {cert};\n        ssl_certificate_key {key};",
            bind = cfg.bind,
            https = cfg.https_port,
            cert = nginx_escape_root(&site.root.parent().unwrap_or(Path::new(".")).join("..").join("etc").join("ssl").join("devkit.pem")).unwrap_or_default(),
            key = ""
        )
    } else {
        format!("listen {bind}:{http};", bind = cfg.bind, http = cfg.http_port)
    };

    let conf = if ssl && !cfg.ssl {
        render_site(cfg, site, false)?
    } else if ssl {
        format!(
            r#"server {{
        listen {bind}:{https} ssl;
        server_name {hostname};
        ssl_certificate {cert};
        ssl_certificate_key {key};
        root {root};
        index index.php index.html;
        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}
        {php_loc}
}}
"#,
            bind = cfg.bind,
            https = cfg.https_port,
            cert = "PLACEHOLDER_CERT",
            key = "PLACEHOLDER_KEY",
            php_loc = php_location(&php),
        )
    } else {
        format!(
            r#"server {{
        listen {bind}:{http};
        server_name {hostname};
        root "{root}";
        index index.php index.html;
        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}
        {php_loc}
}}
"#,
            bind = cfg.bind,
            http = cfg.http_port,
            php_loc = php_location(&php),
        )
    };

    let _ = listen;
    assert_config_localhost_only(&conf)?;
    Ok(conf)
}

pub fn render_site_http(paths: &Paths, cfg: &AppConfig, site: &Site) -> Result<String> {
    let name = nginx_escape_server_name(&site.name)?;
    let hostname = format!("{name}.{}", cfg.tld);
    let root = nginx_escape_root(&site.root)?;
    let php = format!("{}:{}", cfg.bind, cfg.php_port);
    let mut conf = format!(
        r#"server {{
        listen {bind}:{http};
        server_name {hostname};
        root "{root}";
        index index.php index.html;
        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}
        {php_loc}
}}
"#,
        bind = cfg.bind,
        http = cfg.http_port,
        php_loc = php_location(&php),
    );
    if cfg.ssl && paths.ssl_cert().exists() {
        conf.push_str(&format!(
            r#"
server {{
        listen {bind}:{https} ssl;
        server_name {hostname};
        ssl_certificate {cert};
        ssl_certificate_key {key};
        root "{root}";
        index index.php index.html;
        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}
        {php_loc}
}}
"#,
            bind = cfg.bind,
            https = cfg.https_port,
            cert = paths.posix(&paths.ssl_cert()),
            key = paths.posix(&paths.ssl_key()),
            php_loc = php_location(&php),
        ));
    }
    assert_config_localhost_only(&conf)?;
    Ok(conf)
}

pub fn write_fastcgi_params(paths: &Paths) -> Result<()> {
    let p = paths.etc.join("nginx").join("fastcgi_params");
    if p.exists() {
        return Ok(());
    }
    fs::write(
        p,
        r#"fastcgi_param  QUERY_STRING       $query_string;
fastcgi_param  REQUEST_METHOD     $request_method;
fastcgi_param  CONTENT_TYPE       $content_type;
fastcgi_param  CONTENT_LENGTH     $content_length;
fastcgi_param  SCRIPT_NAME        $fastcgi_script_name;
fastcgi_param  REQUEST_URI        $request_uri;
fastcgi_param  DOCUMENT_URI       $document_uri;
fastcgi_param  DOCUMENT_ROOT      $document_root;
fastcgi_param  SERVER_PROTOCOL    $server_protocol;
fastcgi_param  REQUEST_SCHEME     $scheme;
fastcgi_param  HTTPS              $https if_not_empty;
fastcgi_param  GATEWAY_INTERFACE  CGI/1.1;
fastcgi_param  SERVER_SOFTWARE    nginx;
fastcgi_param  REMOTE_ADDR        $remote_addr;
fastcgi_param  REMOTE_PORT        $remote_port;
fastcgi_param  SERVER_ADDR        $server_addr;
fastcgi_param  SERVER_PORT        $server_port;
fastcgi_param  SERVER_NAME        $server_name;
fastcgi_param  REDIRECT_STATUS    200;
"#,
    )?;
    Ok(())
}

pub fn write_mime_types(paths: &Paths) -> Result<()> {
    let p = paths.etc.join("nginx").join("mime.types");
    if p.exists() {
        return Ok(());
    }
    fs::write(
        p,
        r#"types {
    text/html                             html htm shtml;
    text/css                              css;
    text/xml                              xml;
    image/gif                             gif;
    image/jpeg                            jpeg jpg;
    application/javascript                js;
    application/json                      json;
    image/png                             png;
    image/svg+xml                         svg;
    image/webp                            webp;
    application/font-woff                 woff;
    font/woff2                            woff2;
    application/octet-stream              bin exe dll;
    application/zip                       zip;
    application/pdf                       pdf;
    text/plain                            txt;
}
"#,
    )?;
    Ok(())
}

pub fn write_all(paths: &Paths, cfg: &AppConfig, sites: &[Site]) -> Result<()> {
    write_mime_types(paths)?;
    write_fastcgi_params(paths)?;
    fs::create_dir_all(paths.nginx_sites())?;
    fs::create_dir_all(&paths.tmp)?;
    let main = render_main(paths, cfg, sites)?;
    fs::write(paths.nginx_conf(), main)?;
    for old in fs::read_dir(paths.nginx_sites())? {
        let old = old?;
        if old.path().extension().and_then(|e| e.to_str()) == Some("conf") {
            fs::remove_file(old.path())?;
        }
    }
    for site in sites {
        if site.name == "default" {
            continue;
        }
        let body = render_site_http(paths, cfg, site)?;
        fs::write(
            paths.nginx_sites().join(format!("{}.conf", site.name)),
            body,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Site;
    use std::path::PathBuf;

    fn cfg() -> AppConfig {
        AppConfig::default()
    }

    #[test]
    fn generated_listen_is_localhost() {
        let paths = Paths::from_root(PathBuf::from(r"C:\DevKit"));
        let html = render_main(&paths, &cfg(), &[]).unwrap();
        assert!(html.contains("listen 127.0.0.1:8080"));
        assert!(!html.contains("0.0.0.0"));
        assert!(assert_config_localhost_only(&html).is_ok());
    }

    #[test]
    fn vhost_uses_sanitized_name_only() {
        let paths = Paths::from_root(PathBuf::from(r"C:\DevKit"));
        let site = Site {
            name: "blog".into(),
            hostname: "blog.test".into(),
            root: PathBuf::from(r"C:\DevKit\www\blog"),
            php_version: "8.3".into(),
            ssl: false,
            url: "http://blog.test:8080".into(),
        };
        let conf = render_site_http(&paths, &cfg(), &site).unwrap();
        assert!(conf.contains("server_name blog.test"));
        assert!(conf.contains("listen 127.0.0.1:8080"));
        assert!(!conf.contains("0.0.0.0"));
    }

    #[test]
    fn injection_name_cannot_render() {
        let paths = Paths::from_root(PathBuf::from(r"C:\DevKit"));
        let site = Site {
            name: r#"foo"; deny all; #"#.into(),
            hostname: "x".into(),
            root: PathBuf::from(r"C:\DevKit\www\foo"),
            php_version: "8.3".into(),
            ssl: false,
            url: String::new(),
        };
        assert!(render_site_http(&paths, &cfg(), &site).is_err());
    }
}
