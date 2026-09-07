import { useCallback, useEffect, useState } from "react";
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

type Stack = {
  root: string;
  php_version: string;
  services: Service[];
  sites: Site[];
  dns: string;
  warnings: string[];
  port_checks?: PortCheck[];
};

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

  const refresh = useCallback(async () => {
    try {
      const st = await invoke<Stack>("get_status");
      setStack(st);
    } catch (e) {
      setLog(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 2500);
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
      await refresh();
    } catch (e) {
      setLog(String(e));
    } finally {
      setBusy(null);
    }
  }

  const running = stack?.services.some((s) => s.status === "running") ?? false;

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
          <strong>:{stack?.services.find((s) => s.id === "nginx")?.port ?? 8080}</strong>
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
          <h2>Sitios · www</h2>
          <ul className="sites">
            {stack?.sites.map((s) => (
              <li key={s.name}>
                <button
                  className="link"
                  onClick={() => invoke("open_in_browser", { url: s.url })}
                >
                  {s.hostname}
                </button>
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
          <h2>Puertos</h2>
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
        <button
          onClick={() =>
            run("web", async () => {
              const url = await invoke<string>("default_url");
              await invoke("open_in_browser", { url });
              return url;
            })
          }
        >
          Abrir :8080
        </button>
        <button onClick={() => run("ports", () => invoke("check_ports"))}>
          Revisar puertos
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
    </div>
  );
}
