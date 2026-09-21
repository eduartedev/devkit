//! Servidor MCP de solo lectura para logs de DevKit.
//! No se inicia con `devkit start`; solo cuando Cursor/Claude lo lanzan.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use serde::Deserialize;

#[derive(Clone)]
struct DevKitLogs {
    paths: devkit_core::Paths,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TailArgs {
    /// Id del log: `php`, `nginx-error`, `nginx-access`, o ruta bajo www/ (ej. `app/storage/logs/laravel.log`).
    id: String,
    /// Líneas a devolver (1–500, default 80).
    #[serde(default)]
    lines: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchArgs {
    id: String,
    query: String,
    #[serde(default)]
    max_matches: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListArgs {
    /// Si true, incluye logs típicos de frameworks bajo www/.
    #[serde(default)]
    include_projects: Option<bool>,
}

#[tool_router]
impl DevKitLogs {
    fn new(paths: devkit_core::Paths) -> Self {
        Self {
            paths,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Lista logs del kit en logs/ (php, nginx, mariadb, redis, mailpit) y opcionalmente frameworks bajo www/"
    )]
    async fn devkit_logs_list(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let include = args.include_projects.unwrap_or(true);
        match devkit_core::logs::list_logs(&self.paths, include) {
            Ok(entries) => {
                let mut md = String::from("## Logs DevKit\n\n");
                for e in &entries {
                    let mark = if e.exists { "ok" } else { "ausente" };
                    md.push_str(&format!(
                        "- `{}` ({:?}) — {mark} — {} bytes — {}\n",
                        e.id,
                        e.kind,
                        e.size_bytes,
                        e.path.display()
                    ));
                }
                Ok(CallToolResult::success(vec![Content::text(md)]))
            }
            Err(e) => Err(McpError::internal_error(e.to_string(), None)),
        }
    }

    #[tool(description = "Devuelve las últimas líneas de un log (php, nginx-error, o ruta bajo www/)")]
    async fn devkit_logs_tail(
        &self,
        Parameters(args): Parameters<TailArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = match devkit_core::logs::resolve_log_path(&self.paths, &args.id) {
            Ok(p) => p,
            Err(e) => return Err(McpError::invalid_params(e.to_string(), None)),
        };
        match devkit_core::logs::tail_lines(&path, devkit_core::logs::clamp_lines(args.lines)) {
            Ok(text) if text.is_empty() => Ok(CallToolResult::success(vec![Content::text(
                format!("(vacío o inexistente: {})", args.id),
            )])),
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Err(McpError::internal_error(e.to_string(), None)),
        }
    }

    #[tool(description = "Busca texto en un log (case-insensitive); útil para Fatal, Exception, etc.")]
    async fn devkit_logs_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = match devkit_core::logs::resolve_log_path(&self.paths, &args.id) {
            Ok(p) => p,
            Err(e) => return Err(McpError::invalid_params(e.to_string(), None)),
        };
        match devkit_core::logs::search_lines(&path, &args.query, args.max_matches.unwrap_or(50)) {
            Ok(hits) => {
                if hits.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Sin coincidencias para {:?} en {}",
                        args.query, args.id
                    ))]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(hits.join("\n"))]))
                }
            }
            Err(e) => Err(McpError::invalid_params(e.to_string(), None)),
        }
    }
}

#[tool_handler]
impl ServerHandler for DevKitLogs {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "MCP de DevKit (solo lectura). Usa devkit_logs_list / tail / search para ver errores PHP (logs/php_error.log), Nginx y logs de frameworks bajo www/. No forma parte del stack start/stop."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let kit = devkit_core::DevKit::load().map_err(|e| format!("DevKit::load: {e}"))?;
    let service = DevKitLogs::new(kit.paths.clone()).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
