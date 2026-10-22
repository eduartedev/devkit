import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { hasMessage, LANG_OPTIONS, type MessageKey } from "./i18n/index.ts";
import { useI18n } from "./i18n/I18n.tsx";

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

type AutoStartMode = "off" | "all" | "selected";

type AppSettings = {
  tld: string;
  follow_www_junctions: boolean;
  fastcgi_timeout_secs: number;
  launch_at_login: boolean;
  start_hidden: boolean;
  auto_start_mode: AutoStartMode;
  auto_start_services: string[];
};

const AUTO_START_SERVICE_IDS = ["mariadb", "php", "nginx", "redis", "mailpit"] as const;

type Stack = {
  root: string;
  php_version: string;
  services: Service[];
  sites: Site[];
  dns: string;
  warnings: string[];
  port_checks?: PortCheck[];
};

const PORT_FIELDS: { key: keyof PortsSettings; label: MessageKey }[] = [
  { key: "http_port", label: "portHttp" },
  { key: "https_port", label: "portHttps" },
  { key: "php_port", label: "portPhp" },
  { key: "mysql_port", label: "portMysql" },
  { key: "redis_port", label: "portRedis" },
  { key: "mailpit_ui", label: "portMailpitUi" },
  { key: "mailpit_smtp", label: "portMailpitSmtp" },
];

type McpExeInfo = {
  path: string;
  exists: boolean;
  can_build: boolean;
};

function mcpExe(root: string): string {
  const base = root.replace(/[\\/]+$/, "");
  return `${base}\\target\\release\\devkit-mcp.exe`;
}

function mcpCursorJson(exePath: string): string {
  const command = exePath.replace(/\\/g, "\\\\");
  return `{
  "mcpServers": {
    "devkit": {
      "command": "${command}"
    }
  }
}`;
}

function portBadge(
  owner: PortOwner,
  t: (key: MessageKey) => string,
): { label: string; cls: string } {
  switch (owner.kind) {
    case "free":
      return { label: t("badgeFree"), cls: "port-free" };
    case "ours":
      return { label: t("badgeOurs"), cls: "port-ours" };
    case "foreign":
      return { label: t("badgeBusy"), cls: "port-busy" };
    case "unknown":
      return { label: "?", cls: "port-busy" };
  }
}

export default function App() {
  const { lang, setLang, t } = useI18n();
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
  const [moreOpen, setMoreOpen] = useState(false);
  const [mcpOpen, setMcpOpen] = useState(false);
  const [mcpInfo, setMcpInfo] = useState<McpExeInfo | null>(null);
  const [appOpen, setAppOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<"sites" | "php" | "general">("sites");
  const [appDraft, setAppDraft] = useState<AppSettings | null>(null);
  const [appError, setAppError] = useState("");
  const moreRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    try {
      const st = await invoke<Stack>("get_status");
      setStack(st);
    } catch {
      // Durante un trabajo pesado puede devolver caché vacía al inicio; reintenta en el poll.
    }
  }, []);

  useEffect(() => {
    if (!moreOpen) return;
    function onPointerDown(e: PointerEvent) {
      if (moreRef.current && !moreRef.current.contains(e.target as Node)) {
        setMoreOpen(false);
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setMoreOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [moreOpen]);

  useEffect(() => {
    refresh();
    // Poll ligero cada 5s; status usa caché si el kit está ocupado.
    const id = setInterval(refresh, 5000);
    invoke<string[]>("php_versions").then(setPhp).catch(() => {});
    return () => clearInterval(id);
  }, [refresh]);

  /** Acciones pesadas: una a la vez (bloquean Start/Stop/config, no Terminal/URLs). */
  async function runHeavy(label: string, fn: () => Promise<unknown>) {
    if (busy) {
      setLog(t("busyInProgress", { op: operationName(busy) }));
      return;
    }
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

  /** Paralelo acotado: no pone busy global (Terminal, Adminer, abrir sitio). */
  async function runLight(fn: () => Promise<unknown>) {
    try {
      const out = await fn();
      if (typeof out === "string" && out) setLog(out);
    } catch (e) {
      setLog(String(e));
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
      setLog(t("portsSaved"));
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

  async function openAppModal() {
    setAppError("");
    setSettingsTab("sites");
    try {
      const cfg = await invoke<AppSettings>("get_app_settings");
      setAppDraft(cfg);
      setAppOpen(true);
    } catch (e) {
      setLog(String(e));
    }
  }

  async function saveApp() {
    if (!appDraft) return;
    setAppError("");
    setBusy("app-settings");
    setLog("");
    try {
      const msg = await invoke<string>("update_app_settings", { settings: appDraft });
      setAppOpen(false);
      setLog(msg);
      await refresh();
    } catch (e) {
      setAppError(String(e));
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

  function operationName(id: string): string {
    const key = `busy.${id}`;
    return hasMessage(key) ? t(key) : id;
  }

  const running = stack?.services.some((s) => s.status === "running") ?? false;
  const httpPort = stack?.services.find((s) => s.id === "nginx")?.port ?? 8080;

  return (
    <div className="deck">
      <header className="mast">
        <div>
          <p className="kicker">{t("kicker")}</p>
          <h1>DevKit</h1>
        </div>
        <div className="mast-actions">
          <button
            className="go"
            disabled={!!busy}
            onClick={() => runHeavy("start", () => invoke("start_all"))}
          >
            {t("startAll")}
          </button>
          <button
            className="stop"
            disabled={!!busy}
            onClick={() => runHeavy("stop", () => invoke("stop_all"))}
          >
            {t("stopAll")}
          </button>
          <span className={`lamp ${running ? "on" : "off"}`}>
            {running ? t("lampOn") : t("lampOff")}
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
          <h2>{t("services")}</h2>
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
                  <button
                    disabled={!!busy}
                    onClick={() =>
                      runHeavy(s.id, () => invoke("stop_service", { id: s.id }))
                    }
                  >
                    {t("stop")}
                  </button>
                ) : (
                  <button
                    disabled={!!busy}
                    onClick={() =>
                      runHeavy(s.id, () => invoke("start_service", { id: s.id }))
                    }
                  >
                    {t("start")}
                  </button>
                )}
              </li>
            ))}
            {!stack?.services.length && (
              <li className="empty">{t("servicesEmpty")}</li>
            )}
          </ul>
        </section>

        <section className="panel">
          <div className="panel-head">
            <h2>{t("sites")}</h2>
            <button
              type="button"
              disabled={!!busy}
              title={t("regenVhostsTitle")}
              onClick={() => runHeavy("vhosts", () => invoke("regenerate_vhosts"))}
            >
              {t("regenVhosts")}
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
                    title={t("vhostTitle")}
                    onClick={() =>
                      runLight(() => invoke("open_site_vhost", { name: s.name }))
                    }
                  >
                    vhost
                  </button>
                </div>
                <code>{s.url}</code>
              </li>
            ))}
            {!stack?.sites.length && <li className="empty">{t("sitesEmpty")}</li>}
          </ul>
          <form
            className="create"
            onSubmit={(e) => {
              e.preventDefault();
              if (!name.trim()) return;
              runHeavy("create", () =>
                invoke("create_site", { kind, name: name.trim().toLowerCase() }),
              );
            }}
          >
            <select value={kind} onChange={(e) => setKind(e.target.value)}>
              <option value="blank">{t("kindBlank")}</option>
              <option value="laravel">{t("kindLaravel")}</option>
              <option value="wordpress">{t("kindWordpress")}</option>
            </select>
            <input
              placeholder={t("sitePlaceholder")}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <button type="submit" disabled={!!busy}>
              {t("create")}
            </button>
          </form>
        </section>
        <section className="panel">
          <div className="panel-head">
            <h2>{t("ports")}</h2>
            <button type="button" disabled={!!busy} onClick={openPortsModal}>
              {t("edit")}
            </button>
          </div>
          <ul className="ports">
            {(stack?.port_checks ?? []).map((p) => {
              const badge = portBadge(p.owner, t);
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
                      <small>{t("portAlt", { list: p.suggested_ports.join(", ") })}</small>
                    )}
                  </div>
                </li>
              );
            })}
            {!stack?.port_checks?.length && (
              <li className="empty">{t("portsEmpty")}</li>
            )}
          </ul>
        </section>
      </main>

      <section className="tools">
        <button
          type="button"
          title={t("terminalTitle")}
          onClick={() => runLight(() => invoke("open_terminal"))}
        >
          {t("terminal")}
        </button>
        <button
          type="button"
          onClick={() =>
            runLight(async () => {
              const url = await invoke<string>("adminer_url");
              await invoke("open_in_browser", { url });
              return url;
            })
          }
        >
          Adminer
        </button>
        <button
          type="button"
          title={t("mailpitTitle")}
          onClick={() =>
            runLight(async () => {
              const url = await invoke<string>("mailpit_url");
              await invoke("open_in_browser", { url });
              return url;
            })
          }
        >
          Mailpit
        </button>
        <div className="tools-more" ref={moreRef}>
          <button
            type="button"
            className={moreOpen ? "tools-more-btn open" : "tools-more-btn"}
            aria-expanded={moreOpen}
            aria-haspopup="menu"
            onClick={() => setMoreOpen((o) => !o)}
          >
            {t("more")}
          </button>
          {moreOpen && (
            <div className="tools-dropdown" role="menu">
              <p className="tools-group">{t("groupDevkit")}</p>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  openAppModal();
                }}
              >
                {t("settings")}
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  setMcpOpen(true);
                  void invoke<McpExeInfo>("mcp_exe_status")
                    .then(setMcpInfo)
                    .catch(() => setMcpInfo(null));
                }}
              >
                {t("mcp")}
              </button>

              <p className="tools-group">{t("groupDb")}</p>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  openDbModal();
                }}
              >
                {t("credentials")}
              </button>

              <p className="tools-group">{t("groupPhp")}</p>
              {php.length > 0 ? (
                <select
                  className="tools-php"
                  value={stack?.php_version}
                  disabled={!!busy}
                  aria-label={t("phpVersion")}
                  onChange={(e) => {
                    setMoreOpen(false);
                    runHeavy("php", () => invoke("use_php", { version: e.target.value }));
                  }}
                >
                  {php.map((v) => (
                    <option key={v} value={v}>
                      PHP {v}
                    </option>
                  ))}
                </select>
              ) : (
                <p className="tools-empty">{t("noPhp")}</p>
              )}
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  openExtModal();
                }}
              >
                {t("extensions")}
              </button>

              <p className="tools-group">{t("groupNet")}</p>
              <button
                type="button"
                role="menuitem"
                title={`Abrir http://127.0.0.1:${httpPort}/`}
                onClick={() => {
                  setMoreOpen(false);
                  runLight(async () => {
                    const url = await invoke<string>("default_url");
                    await invoke("open_in_browser", { url });
                    return url;
                  });
                }}
              >
                {t("openLocal", { port: httpPort })}
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!!busy}
                onClick={() => {
                  setMoreOpen(false);
                  runHeavy("ports", () => invoke("check_ports"));
                }}
              >
                {t("checkPorts")}
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  openPortsModal();
                }}
              >
                {t("editPorts")}
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!!busy}
                title={t("updateHostsTitle")}
                onClick={() => {
                  setMoreOpen(false);
                  runHeavy("hosts", () => invoke("ensure_hosts"));
                }}
              >
                {t("updateHosts")}
              </button>

              <p className="tools-group">{t("groupSystem")}</p>
              <button
                type="button"
                role="menuitem"
                disabled={!!busy}
                onClick={() => {
                  setMoreOpen(false);
                  runHeavy("ssl", () => invoke("enable_ssl"));
                }}
              >
                {t("ssl")}
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!!busy}
                onClick={() => {
                  setMoreOpen(false);
                  runHeavy("pack", () => invoke("pack_portable"));
                }}
              >
                {t("zip")}
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!!busy}
                onClick={() => {
                  setMoreOpen(false);
                  runHeavy("setup", () => invoke("setup_stack", { full: true }));
                }}
              >
                {t("setupFull")}
              </button>
            </div>
          )}
        </div>
      </section>

      {(busy || log || stack?.warnings?.length) && (
        <pre className="log">
          {busy ? `… ${operationName(busy)}\n` : ""}
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
            <h2 id="ports-modal-title">{t("portsTitle")}</h2>
            <p className="modal-hint">{t("portsHint")}</p>
            <div className="port-form">
              {PORT_FIELDS.map(({ key, label }) => (
                <label key={key}>
                  <span>{t(label)}</span>
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
                {t("cancel")}
              </button>
              <button
                type="button"
                className="go"
                disabled={!!busy}
                onClick={savePorts}
              >
                {t("saveRestart")}
              </button>
            </div>
          </div>
        </div>
      )}

      {appOpen && appDraft && (
        <div
          className="modal-backdrop"
          role="presentation"
          onClick={() => !busy && setAppOpen(false)}
        >
          <div
            className="modal modal-settings"
            role="dialog"
            aria-modal="true"
            aria-labelledby="app-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <header className="settings-head">
              <p className="settings-kicker">{t("settingsKicker")}</p>
              <h2 id="app-modal-title">{t("settingsTitle")}</h2>
              <p className="modal-hint">{t("settingsHint")}</p>
            </header>

            <div className="settings-tabs" role="tablist" aria-label={t("settingsTitle")}>
              <button
                type="button"
                role="tab"
                id="settings-tab-sites"
                aria-selected={settingsTab === "sites"}
                aria-controls="settings-panel-sites"
                className={settingsTab === "sites" ? "on" : undefined}
                onClick={() => setSettingsTab("sites")}
              >
                {t("tabSites")}
              </button>
              <button
                type="button"
                role="tab"
                id="settings-tab-php"
                aria-selected={settingsTab === "php"}
                aria-controls="settings-panel-php"
                className={settingsTab === "php" ? "on" : undefined}
                onClick={() => setSettingsTab("php")}
              >
                {t("tabPhp")}
              </button>
              <button
                type="button"
                role="tab"
                id="settings-tab-general"
                aria-selected={settingsTab === "general"}
                aria-controls="settings-panel-general"
                className={settingsTab === "general" ? "on" : undefined}
                onClick={() => setSettingsTab("general")}
              >
                {t("tabGeneral")}
              </button>
            </div>

            <div className="settings-body">
            <div
              role="tabpanel"
              id="settings-panel-sites"
              aria-labelledby="settings-tab-sites"
              className={settingsTab === "sites" ? "is-active" : undefined}
              inert={settingsTab === "sites" ? undefined : true}
            >
            <section className="settings-section">
              <h3 className="settings-section-title">{t("sectionSites")}</h3>
              <div className="settings-row">
                <div className="settings-copy">
                  <span className="settings-label">{t("tld")}</span>
                  <p className="settings-desc">
                    {t("tldBefore")}
                    <code>
                      {t("tldHost")}
                      <b>{appDraft.tld.trim() || "…"}</b>
                    </code>
                    {t("tldAfter")}
                  </p>
                </div>
                <div className="settings-control settings-control-tld">
                  <span className="tld-prefix">*.</span>
                  <input
                    value={appDraft.tld}
                    disabled={!!busy}
                    placeholder="test"
                    aria-label={t("tldAria")}
                    onChange={(e) =>
                      setAppDraft((prev) =>
                        prev ? { ...prev, tld: e.target.value } : prev,
                      )
                    }
                  />
                </div>
              </div>
            </section>

            <section className="settings-section">
              <h3 className="settings-section-title">{t("sectionWww")}</h3>
              <div className="settings-row">
                <div className="settings-copy">
                  <span className="settings-label">{t("junctions")}</span>
                  <p className="settings-desc">
                    {t("junctionsBefore")}
                    <code>www/</code>
                    {t("junctionsAfter")}
                  </p>
                </div>
                <div className="settings-control">
                  <button
                    type="button"
                    role="switch"
                    className={
                      appDraft.follow_www_junctions ? "switch on" : "switch"
                    }
                    aria-checked={appDraft.follow_www_junctions}
                    disabled={!!busy}
                    onClick={() =>
                      setAppDraft((prev) =>
                        prev
                          ? {
                              ...prev,
                              follow_www_junctions: !prev.follow_www_junctions,
                            }
                          : prev,
                      )
                    }
                  >
                    <span className="switch-knob" />
                    <span className="switch-text">
                      {appDraft.follow_www_junctions ? t("yes") : t("no")}
                    </span>
                  </button>
                </div>
              </div>
            </section>
            </div>

            <div
              role="tabpanel"
              id="settings-panel-php"
              aria-labelledby="settings-tab-php"
              className={settingsTab === "php" ? "is-active" : undefined}
              inert={settingsTab === "php" ? undefined : true}
            >
            <section className="settings-section">
              <h3 className="settings-section-title">{t("sectionPhp")}</h3>
              <div className="settings-row">
                <div className="settings-copy">
                  <span className="settings-label">{t("nginxWait")}</span>
                  <p className="settings-desc">
                    {t("nginxWaitBefore")}
                    <code>fastcgi_read_timeout</code>
                    {t("nginxWaitAnd")}
                    <code>fastcgi_send_timeout</code>
                    {t("nginxWaitMid")}
                    <code>location ~ \.php$</code>
                    {t("nginxWaitAfter")}
                  </p>
                </div>
                <div className="settings-control">
                  <input
                    className="settings-timeout"
                    type="number"
                    min={1}
                    max={7200}
                    step={1}
                    value={appDraft.fastcgi_timeout_secs}
                    disabled={!!busy}
                    aria-label={t("nginxWaitAria")}
                    onChange={(e) => {
                      const n = Number(e.target.value);
                      setAppDraft((prev) =>
                        prev && Number.isFinite(n)
                          ? { ...prev, fastcgi_timeout_secs: Math.trunc(n) }
                          : prev,
                      );
                    }}
                  />
                </div>
              </div>
            </section>
            </div>

            <div
              role="tabpanel"
              id="settings-panel-general"
              aria-labelledby="settings-tab-general"
              className={settingsTab === "general" ? "is-active" : undefined}
              inert={settingsTab === "general" ? undefined : true}
            >
            <section className="settings-section">
              <h3 className="settings-section-title">{t("sectionStartup")}</h3>
              <div className="settings-row">
                <div className="settings-copy">
                  <span className="settings-label">{t("launchAtLogin")}</span>
                  <p className="settings-desc">{t("launchAtLoginDesc")}</p>
                </div>
                <div className="settings-control">
                  <button
                    type="button"
                    role="switch"
                    className={appDraft.launch_at_login ? "switch on" : "switch"}
                    aria-checked={appDraft.launch_at_login}
                    disabled={!!busy}
                    onClick={() =>
                      setAppDraft((prev) =>
                        prev ? { ...prev, launch_at_login: !prev.launch_at_login } : prev,
                      )
                    }
                  >
                    <span className="switch-knob" />
                    <span className="switch-text">
                      {appDraft.launch_at_login ? t("yes") : t("no")}
                    </span>
                  </button>
                </div>
              </div>
              <div className="settings-row">
                <div className="settings-copy">
                  <span className="settings-label">{t("startHidden")}</span>
                  <p className="settings-desc">{t("startHiddenDesc")}</p>
                </div>
                <div className="settings-control">
                  <button
                    type="button"
                    role="switch"
                    className={appDraft.start_hidden ? "switch on" : "switch"}
                    aria-checked={appDraft.start_hidden}
                    disabled={!!busy}
                    onClick={() =>
                      setAppDraft((prev) =>
                        prev ? { ...prev, start_hidden: !prev.start_hidden } : prev,
                      )
                    }
                  >
                    <span className="switch-knob" />
                    <span className="switch-text">
                      {appDraft.start_hidden ? t("yes") : t("no")}
                    </span>
                  </button>
                </div>
              </div>
              <div className="settings-row">
                <div className="settings-copy">
                  <span className="settings-label">{t("autoStartLabel")}</span>
                  <p className="settings-desc">{t("autoStartDesc")}</p>
                </div>
                <div className="settings-control">
                  <select
                    className="settings-lang"
                    value={appDraft.auto_start_mode}
                    aria-label={t("autoStartLabel")}
                    disabled={!!busy}
                    onChange={(e) => {
                      const mode = e.target.value as AutoStartMode;
                      setAppDraft((prev) =>
                        prev ? { ...prev, auto_start_mode: mode } : prev,
                      );
                    }}
                  >
                    <option value="off">{t("autoStartOff")}</option>
                    <option value="all">{t("autoStartAll")}</option>
                    <option value="selected">{t("autoStartSelected")}</option>
                  </select>
                </div>
              </div>
              {appDraft.auto_start_mode === "selected" && (
                <div className="settings-row settings-row-stack">
                  <ul className="auto-start-services">
                    {AUTO_START_SERVICE_IDS.map((id) => {
                      const svc = stack?.services.find((s) => s.id === id);
                      const label = svc?.display_name ?? id;
                      const on = appDraft.auto_start_services.includes(id);
                      return (
                        <li key={id}>
                          <label>
                            <input
                              type="checkbox"
                              checked={on}
                              disabled={!!busy}
                              onChange={() =>
                                setAppDraft((prev) => {
                                  if (!prev) return prev;
                                  const next = prev.auto_start_services.includes(id)
                                    ? prev.auto_start_services.filter((x) => x !== id)
                                    : [...prev.auto_start_services, id];
                                  return { ...prev, auto_start_services: next };
                                })
                              }
                            />
                            {label}
                          </label>
                        </li>
                      );
                    })}
                  </ul>
                </div>
              )}
            </section>
            <section className="settings-section">
              <h3 className="settings-section-title">{t("tabGeneral")}</h3>
              <div className="settings-row">
                <div className="settings-copy">
                  <span className="settings-label">{t("langLabel")}</span>
                  <p className="settings-desc">{t("langDesc")}</p>
                </div>
                <div className="settings-control">
                  <select
                    className="settings-lang"
                    value={lang}
                    aria-label={t("langLabel")}
                    onChange={(e) => {
                      const next = LANG_OPTIONS.find((option) => option.id === e.target.value);
                      if (next) setLang(next.id);
                    }}
                  >
                    {LANG_OPTIONS.map((option) => (
                      <option key={option.id} value={option.id}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            </section>
            </div>
            </div>

            {appError && <p className="modal-error">{appError}</p>}
            <div className="modal-actions">
              <button type="button" disabled={!!busy} onClick={() => setAppOpen(false)}>
                {t("cancel")}
              </button>
              <button type="button" className="go" disabled={!!busy} onClick={saveApp}>
                {t("save")}
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
            <h2 id="db-modal-title">{t("dbTitle")}</h2>
            <p className="modal-hint">
              {t("dbHintBefore")}
              <code>root</code>
              {t("dbHintAfter")}
            </p>
            <div className="db-form">
              <label>
                <span>{t("user")}</span>
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
                <span>{t("password")}</span>
                <div className="db-pass-row">
                  <input
                    type={showDbPassword ? "text" : "password"}
                    autoComplete="new-password"
                    value={dbDraft.password}
                    disabled={!!busy}
                    placeholder={t("passwordPlaceholder")}
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
                    {showDbPassword ? t("hide") : t("show")}
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
                {t("resetDefault")}
              </button>
              <div style={{ display: "flex", gap: 8 }}>
                <button type="button" disabled={!!busy} onClick={() => setDbOpen(false)}>
                  {t("cancel")}
                </button>
                <button type="button" className="go" disabled={!!busy} onClick={saveDb}>
                  {t("save")}
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
            <h2 id="ext-modal-title">{t("extTitle")}</h2>
            <p className="modal-hint">
              {t("extHintBefore")}
              <code>ext/*.dll</code>
              {t("extHintMid")}
              <code>extension=…</code>
              {t("extHintAfter")}
            </p>
            <input
              className="ext-filter"
              placeholder={t("filterPlaceholder")}
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
                <p className="empty">{t("extEmpty")}</p>
              )}
            </div>
            {extError && <p className="modal-error">{extError}</p>}
            <div className="modal-actions">
              <button type="button" disabled={!!busy} onClick={() => setExtOpen(false)}>
                {t("cancel")}
              </button>
              <button type="button" className="go" disabled={!!busy} onClick={saveExt}>
                {t("saveRestartPhp")}
              </button>
            </div>
          </div>
        </div>
      )}

      {mcpOpen && (
        <div className="modal-backdrop" onClick={() => setMcpOpen(false)} role="presentation">
          <div
            className="modal modal-wide"
            role="dialog"
            aria-modal="true"
            aria-labelledby="mcp-modal-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 id="mcp-modal-title">{t("mcpTitle")}</h2>
            <p className="modal-hint">
              {t("mcpHintBefore")}
              <code>logs/</code>
              {t("mcpHintAfter")}
            </p>
            <ol className="mcp-steps">
              <li>
                {mcpInfo?.exists
                  ? t("mcpStepReady")
                  : mcpInfo?.can_build
                    ? t("mcpStepBuild")
                    : t("mcpStepMissing")}
              </li>
              <li>
                {t("mcpStepJsonBefore")}
                <code>.cursor/mcp.json</code>
                {t("mcpStepJsonAfter")}
              </li>
              <li>{t("mcpStepTools")}</li>
            </ol>
            {mcpInfo && !mcpInfo.exists && !mcpInfo.can_build && (
              <p className="modal-error">{t("mcpBuildError")}</p>
            )}
            <pre className="mcp-snippet">{mcpCursorJson(mcpInfo?.path ?? mcpExe(stack?.root ?? "C:\\DevKit"))}</pre>
            <p className="modal-hint">
              {t("mcpCommand")} <code>{mcpInfo?.path ?? mcpExe(stack?.root ?? "C:\\DevKit")}</code>
            </p>
            <p className="modal-hint">
              {t("mcpCliBefore")} <code>devkit logs list</code>, <code>devkit logs tail php</code>, <code>devkit logs search nginx-error emerg</code>.
            </p>
            <div className="modal-actions">
              <button type="button" onClick={() => setMcpOpen(false)}>
                {t("close")}
              </button>
              {!mcpInfo?.exists && mcpInfo?.can_build && (
                <button
                  type="button"
                  className="go"
                  disabled={!!busy}
                  onClick={() =>
                    void runHeavy("mcp", async () => {
                      const msg = await invoke<string>("build_mcp");
                      const info = await invoke<McpExeInfo>("mcp_exe_status");
                      setMcpInfo(info);
                      return msg;
                    })
                  }
                >
                  {busy === "mcp" ? t("generating") : t("generateExe")}
                </button>
              )}
              <button
                type="button"
                className="go"
                onClick={() => {
                  const json = mcpCursorJson(
                    mcpInfo?.path ?? mcpExe(stack?.root ?? "C:\\DevKit")
                  );
                  void navigator.clipboard.writeText(json).then(
                    () => setLog(t("mcpCopied")),
                    () => setLog(t("mcpCopyFail"))
                  );
                }}
              >
                {t("copyJson")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
