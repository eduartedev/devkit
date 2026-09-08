import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Status = "stopped" | "starting" | "running" | "crashed";

type Service = {
  id: string;
  display_name: string;
  status: Status;
  bind: string;
  port: number;
  pid: number | null;
  detail: string;
};

type Site = {
  name: string;
  hostname: string;
  root: string;
  php_version: string;
  ssl: boolean;
  url: string;
};

type PortOwner =
  | { kind: "free" }
  | { kind: "ours"; pid: number; name: string }
  | { kind: "foreign"; processes: { pid: number; name: string }[] }
  | { kind: "unknown" };

type PortCheck = {
  service: string;
  display_name: string;
  bind: string;
  port: number;
  owner: PortOwner;
  hint: string;
  suggested_ports: number[];
};

type PortsSettings = {
  http_port: number;
  https_port: number;
  php_port: number;
  mysql_port: number;
  redis_port: number;
  mailpit_smtp: number;
  mailpit_ui: number;
};

type DbCredentials = {
  user: string;
  password: string;
};

type PhpExtensionInfo = {
  name: string;
  dll: string;
  enabled: boolean;
};

type Stack = {
  root: string;
  php_version: string;
  services: Service[];
  sites: Site[];
  dns: string;
  warnings: string[];
  port_checks?: PortCheck[];
};

const PORT_FIELDS: { key: keyof PortsSettings; label: string }[] = [
  { key: "http_port", label: "HTTP (Nginx)" },
  { key: "https_port", label: "HTTPS" },
  { key: "php_port", label: "PHP-FPM" },
  { key: "mysql_port", label: "MariaDB" },
  { key: "redis_port", label: "Redis" },
  { key: "mailpit_ui", label: "Mailpit UI" },
  { key: "mailpit_smtp", label: "Mailpit SMTP" },
];

function portBadge(owner: PortOwner): { label: string; cls: string } {
  switch (owner.kind) {
    case "free":
      return { label: "libre", cls: "port-free" };
    case "ours":
      return { label: "DevKit", cls: "port-ours" };
    case "foreign":
      return { label: "ocupado", cls: "port-busy" };
    case "unknown":
      return { label: "?", cls: "port-busy" };
  }
}

export default function App() {
  const [stack, setStack] = useState<Stack | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [log, setLog] = useState<string>("");
  const [kind, setKind] = useState("blank");
  const [name, setName] = useState("");
  const [php, setPhp] = useState<string[]>([]);
  const [portsOpen, setPortsOpen] = useState(false);
  const [portsDraft, setPortsDraft] = useState<PortsSettings | null>(null);
  const [portsError, setPortsError] = useState("");
  const [dbOpen, setDbOpen] = useState(false);
  const [dbDraft, setDbDraft] = useState<DbCredentials | null>(null);
  const [dbError, setDbError] = useState("");
  const [showDbPassword, setShowDbPassword] = useState(false);
  const [extOpen, setExtOpen] = useState(false);
  const [extList, setExtList] = useState<PhpExtensionInfo[]>([]);
  const [extError, setExtError] = useState("");
  const [extFilter, setExtFilter] = useState("");

  const busyRef = useRef(busy);
  busyRef.current = busy;

  const refresh = useCallback(async () => {
    if (busyRef.current) return;
    try {
      const st = await invoke<Stack>("get_status");
      setStack(st);
    } catch (e) {
      setLog(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    // Poll ligero cada 5s; no solapar con acciones (Start/Stop/…).
    const id = setInterval(refresh, 5000);
    invoke<string[]>("php_versions").then(setPhp).catch(() => {});
    return () => clearInterval(id);
  }, [refresh]);

  async function run(label: string, fn: () => Promise<unknown>) {
    setBusy(label);
    setLog("");
    try {
      const out = await fn();
      if (Array.isArray(out)) setLog(out.join("\n"));
      else if (typeof out === "string") setLog(out);
      else if (out && typeof out === "object" && "services" in (out as object)) {
        setStack(out as Stack);
      }
      await refresh();
    } catch (e) {
      setLog(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function openPortsModal() {
    setPortsError("");
    try {
      const cfg = await invoke<PortsSettings>("get_ports_config");
      setPortsDraft(cfg);
      setPortsOpen(true);
    } catch (e) {
      setLog(String(e));
    }
  }

  async function savePorts() {
    if (!portsDraft) return;
    setPortsError("");
    setBusy("ports-save");
    setLog("");
    try {
      const st = await invoke<Stack>("update_ports", { ports: portsDraft });
      setStack(st);
      setPortsOpen(false);
      setLog("Puertos guardados y servicios reiniciados.");
      await refresh();
    } catch (e) {
      setPortsError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function openDbModal() {
    setDbError("");
    setShowDbPassword(false);
    try {
      const cfg = await invoke<DbCredentials>("get_db_credentials");
      setDbDraft(cfg);
      setDbOpen(true);
    } catch (e) {
      setLog(String(e));
    }
  }

  async function saveDb() {
    if (!dbDraft) return;
    setDbError("");
    setBusy("db-save");
    setLog("");
    try {
      const msg = await invoke<string>("update_db_credentials", { creds: dbDraft });
      setDbOpen(false);
      setLog(msg);
      await refresh();
    } catch (e) {
      setDbError(String(e));
    } finally {
      setBusy(null);
    }
  }

  async function openExtModal() {
    setExtError("");
    setExtFilter("");
    try {
      const list = await invoke<PhpExtensionInfo[]>("list_php_extensions");
      setExtList(list);
      setExtOpen(true);
    } catch (e) {
      setLog(String(e));
    }
  }

  async function saveExt() {
    setExtError("");
    setBusy("php-ext");
    setLog("");
    try {
      const extensions = extList.filter((e) => e.enabled).map((e) => e.name);
      const msg = await invoke<string>("set_php_extensions", { extensions });
      setExtOpen(false);
      setLog(msg);
      await refresh();
    } catch (e) {
      setExtError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const running = stack?.services.some((s) => s.status === "running") ?? false;
  const httpPort = stack?.services.find((s) => s.id === "nginx")?.port ?? 8080;

  return (
    <div className="deck">
      <header className="mast">
        <div>
          <p className="kicker">LOCAL STACK · 127.0.0.1</p>
          <h1>DevKit</h1>
        </div>
        <div className="mast-actions">
          <button
            className="go"
            disabled={!!busy}
            onClick={() => run("start", () => invoke("start_all"))}
          >
            Start All
          </button>
          <button
            className="stop"
            disabled={!!busy}
            onClick={() => run("stop", () => invoke("stop_all"))}
          >
            Stop All
          </button>
          <span className={`lamp ${running ? "on" : "off"}`}>
            {running ? "EN AIRE" : "SILENCIO"}
          </span>
        </div>
      </header>

      <section className="strip">
        <div>
          <span>HTTP</span>
          <strong>:{httpPort}</strong>
        </div>
        <div>
          <span>PHP</span>
          <strong>{stack?.php_version ?? "—"}</strong>
        </div>
        <div>
          <span>DNS</span>
          <strong>{stack?.dns ?? "—"}</strong>
        </div>
        <div className="path" title={stack?.root}>
          <span>ROOT</span>
          <strong>{stack?.root ?? "C:\\DevKit"}</strong>
        </div>
      </section>

      <main className="grid">
        <section className="panel">
          <h2>Servicios</h2>
          <ul className="services">
            {stack?.services.map((s) => (
              <li key={s.id}>
                <i className={s.status} />
                <div>
                  <b>{s.display_name}</b>
                  <small>
                    {s.bind}:{s.port} · {s.detail}
                  </small>
                </div>
                {s.status === "running" ? (
                  <button onClick={() => run(s.id, () => invoke("stop_service", { id: s.id }))}>
                    stop
                  </button>
                ) : (
                  <button onClick={() => run(s.id, () => invoke("start_service", { id: s.id }))}>
                    start
                  </button>
                )}
              </li>
            ))}
            {!stack?.services.length && (
              <li className="empty">Aún no hay binarios. Ejecuta setup.</li>
            )}
          </ul>
        </section>

        <section className="panel">
          <div className="panel-head">
            <h2>Sitios · www</h2>
            <button
              type="button"
              disabled={!!busy}
              title="Reescribe etc/nginx/sites (Laravel → public/)"
              onClick={() => run("vhosts", () => invoke("regenerate_vhosts"))}
            >
              Regenerar vhosts
            </button>
          </div>
          <ul className="sites">
            {stack?.sites.map((s) => (
              <li key={s.name}>
                <div className="site-row">
                  <button
                    className="link"
                    onClick={() => invoke("open_in_browser", { url: s.url })}
                  >
                    {s.hostname}
                  </button>
                  <button
                    type="button"
                    className="mini"
                    disabled={!!busy}
                    title="Editar vhost en bloc de notas"
                    onClick={() =>
                      run("vhost", () => invoke("open_site_vhost", { name: s.name }))
                    }
                  >
                    vhost
                  </button>
                </div>
                <code>{s.url}</code>
              </li>
            ))}
            {!stack?.sites.length && <li className="empty">Crea una carpeta en www/</li>}
          </ul>
          <form
            className="create"
            onSubmit={(e) => {
              e.preventDefault();
              if (!name.trim()) return;
              run("create", () =>
                invoke("create_site", { kind, name: name.trim().toLowerCase() }),
              );
            }}
          >
            <select value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="blank">PHP en blanco</option>
              <option value="laravel">Laravel</option>
              <option value="wordpress">WordPress</option>
            </select>
            <input
              placeholder="nombre-del-sitio"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <button type="submit">Crear</button>
          </form>
        </section>
        <section className="panel">
          <div className="panel-head">
            <h2>Puertos</h2>
            <button type="button" disabled={!!busy} onClick={openPortsModal}>
              Editar
            </button>
          </div>
          <ul className="ports">
            {(stack?.port_checks ?? []).map((p) => {
              const badge = portBadge(p.owner);
              const who =
                p.owner.kind === "foreign"
                  ? p.owner.processes.map((x) => `${x.name}#${x.pid}`).join(", ")
                  : p.owner.kind === "ours"
                    ? `${p.owner.name}#${p.owner.pid}`
                    : p.hint;
              return (
                <li key={p.service}>
                  <span className={`port-badge ${badge.cls}`}>{badge.label}</span>
                  <div>
                    <b>
                      {p.display_name} :{p.port}
                    </b>
                    <small>{who}</small>
                    {p.owner.kind === "foreign" && p.suggested_ports.length > 0 && (
                      <small>alternativas: {p.suggested_ports.join(", ")}</small>
                    )}
                  </div>
                </li>
              );
            })}
            {!stack?.port_checks?.length && (
              <li className="empty">Sin datos de puertos aún</li>
            )}
          </ul>
        </section>
      </main>

      <section className="tools">
        <button onClick={() => run("term", () => invoke("open_terminal"))}>
          Terminal PATH aislado
        </button>
        <button
          onClick={() =>
            run("adminer", async () => {
              const url = await invoke<string>("adminer_url");
              await invoke("open_in_browser", { url });
              return url;
            })
          }
        >
          Adminer
        </button>
        <button type="button" disabled={!!busy} onClick={openDbModal}>
          Base de datos
        </button>
        <button type="button" disabled={!!busy} onClick={openExtModal}>
          Extensiones PHP
        </button>
        <button
          onClick={() =>
            run("web", async () => {
              const url = await invoke<string>("default_url");
              await invoke("open_in_browser", { url });
              return url;
            })
          }
        >
          Abrir :{httpPort}
        </button>
        <button onClick={() => run("ports", () => invoke("check_ports"))}>
          Revisar puertos
        </button>
        <button type="button" disabled={!!busy} onClick={openPortsModal}>
          Editar puertos
        </button>
        <button
          type="button"
          disabled={!!busy}
          title="Solo actualiza el bloque # BEGIN DEVKIT. Puede pedir UAC."
          onClick={() => run("hosts", () => invoke("ensure_hosts"))}
        >
          Actualizar hosts
        </button>
        <button onClick={() => run("ssl", () => invoke("enable_ssl"))}>SSL mkcert</button>
        <button onClick={() => run("pack", () => invoke("pack_portable"))}>Zip portable</button>
        <button onClick={() => run("setup", () => invoke("setup_stack", { full: true }))}>
          Setup completo
        </button>
        {php.length > 0 && (
          <select
            value={stack?.php_version}
            onChange={(e) => run("php", () => invoke("use_php", { version: e.target.value }))}
          >
            {php.map((v) => (
              <option key={v} value={v}>
                PHP {v}
              </option>
            ))}
          </select>
        )}
      </section>

      {(busy || log || stack?.warnings?.length) && (
        <pre className="log">
          {busy ? `… ${busy}\n` : ""}
          {stack?.warnings?.join("\n")}
          {log}
        </pre>
      )}

      {portsOpen && portsDraft && (
        <div
          className="modal-backdrop"
          role="presentation"
          onClick={() => !busy && setPortsOpen(false)}
        >
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="ports-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 id="ports-modal-title">Editar puertos</h2>
            <p className="modal-hint">
              Al guardar se actualiza config.toml, se regeneran configs y se reinician los
              servicios.
            </p>
            <div className="port-form">
              {PORT_FIELDS.map(({ key, label }) => (
                <label key={key}>
                  <span>{label}</span>
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={portsDraft[key]}
                    disabled={!!busy}
                    onChange={(e) => {
                      const n = Number(e.target.value);
                      setPortsDraft((prev) =>
                        prev
                          ? {
                              ...prev,
                              [key]: Number.isFinite(n) ? Math.min(65535, Math.max(0, n)) : 0,
                            }
                          : prev,
                      );
                    }}
                  />
                </label>
              ))}
            </div>
            {portsError && <p className="modal-error">{portsError}</p>}
            <div className="modal-actions">
              <button type="button" disabled={!!busy} onClick={() => setPortsOpen(false)}>
                Cancelar
              </button>
              <button
                type="button"
                className="go"
                disabled={!!busy}
                onClick={savePorts}
              >
                Guardar y reiniciar
              </button>
            </div>
          </div>
        </div>
      )}

      {dbOpen && dbDraft && (
        <div
          className="modal-backdrop"
          role="presentation"
          onClick={() => !busy && setDbOpen(false)}
        >
          <div
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="db-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 id="db-modal-title">Base de datos</h2>
            <p className="modal-hint">
              Por defecto: usuario <code>root</code> sin contraseña (solo 127.0.0.1). Puedes
              cambiarlos; se guardan en secrets (fuera de git) y se aplican a MariaDB si está
              en marcha.
            </p>
            <div className="db-form">
              <label>
                <span>Usuario</span>
                <input
                  type="text"
                  autoComplete="off"
                  spellCheck={false}
                  value={dbDraft.user}
                  disabled={!!busy}
                  onChange={(e) =>
                    setDbDraft((prev) => (prev ? { ...prev, user: e.target.value } : prev))
                  }
                />
              </label>
              <label>
                <span>Contraseña</span>
                <div className="db-pass-row">
                  <input
                    type={showDbPassword ? "text" : "password"}
                    autoComplete="new-password"
                    value={dbDraft.password}
                    disabled={!!busy}
                    placeholder="(vacía = sin contraseña)"
                    onChange={(e) =>
                      setDbDraft((prev) =>
                        prev ? { ...prev, password: e.target.value } : prev,
                      )
                    }
                  />
                  <button
                    type="button"
                    disabled={!!busy}
                    onClick={() => setShowDbPassword((v) => !v)}
                  >
                    {showDbPassword ? "Ocultar" : "Ver"}
                  </button>
                </div>
              </label>
            </div>
            <div className="modal-actions" style={{ justifyContent: "space-between" }}>
              <button
                type="button"
                disabled={!!busy}
                onClick={() =>
                  setDbDraft((prev) =>
                    prev ? { user: "root", password: "" } : prev,
                  )
                }
              >
                Restablecer default
              </button>
              <div style={{ display: "flex", gap: 8 }}>
                <button type="button" disabled={!!busy} onClick={() => setDbOpen(false)}>
                  Cancelar
                </button>
                <button type="button" className="go" disabled={!!busy} onClick={saveDb}>
                  Guardar
                </button>
              </div>
            </div>
            {dbError && <p className="modal-error">{dbError}</p>}
          </div>
        </div>
      )}

      {extOpen && (
        <div
          className="modal-backdrop"
          role="presentation"
          onClick={() => !busy && setExtOpen(false)}
        >
          <div
            className="modal modal-wide"
            role="dialog"
            aria-modal="true"
            aria-labelledby="ext-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 id="ext-modal-title">Extensiones PHP</h2>
            <p className="modal-hint">
              Lista de <code>ext/*.dll</code>. Si un DLL no está en php.ini, PHP no lo carga; al
              activarlo aquí se escribe <code>extension=…</code> y se reinicia PHP-FPM.
            </p>
            <input
              className="ext-filter"
              placeholder="filtrar…"
              value={extFilter}
              onChange={(e) => setExtFilter(e.target.value)}
              disabled={!!busy}
            />
            <div className="ext-list">
              {extList
                .filter(
                  (e) =>
                    !extFilter.trim() ||
                    e.name.includes(extFilter.trim().toLowerCase()) ||
                    e.dll.toLowerCase().includes(extFilter.trim().toLowerCase()),
                )
                .map((e) => (
                  <label key={e.name} className="ext-item">
                    <input
                      type="checkbox"
                      checked={e.enabled}
                      disabled={!!busy}
                      onChange={() =>
                        setExtList((prev) =>
                          prev.map((x) =>
                            x.name === e.name ? { ...x, enabled: !x.enabled } : x,
                          ),
                        )
                      }
                    />
                    <span>
                      <b>{e.name}</b>
                      <small>{e.dll}</small>
                    </span>
                  </label>
                ))}
              {!extList.length && (
                <p className="empty">No hay DLL en ext/ (¿PHP instalado?)</p>
              )}
            </div>
            {extError && <p className="modal-error">{extError}</p>}
            <div className="modal-actions">
              <button type="button" disabled={!!busy} onClick={() => setExtOpen(false)}>
                Cancelar
              </button>
              <button type="button" className="go" disabled={!!busy} onClick={saveExt}>
                Guardar y reiniciar PHP
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
