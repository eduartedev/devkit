import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { es, normalizeLang, resolveText, sameMessageKeys, translate } from "../src/i18n/index.ts";

describe("traducciones del panel", () => {
  it("el inglés tiene exactamente las claves del español", () => {
    assert.equal(sameMessageKeys(), true);
  });

  it("sin idioma guardado usa español", () => {
    assert.equal(normalizeLang(null), "es");
    assert.equal(normalizeLang(""), "es");
    assert.equal(normalizeLang("fr"), "es");
    assert.equal(normalizeLang("en"), "en");
  });

  it("si falta una traducción se muestra el español", () => {
    assert.equal(translate("en", "services"), "Services");
    assert.equal(translate("xx", "services"), es.services);
    assert.equal(translate("es", "services"), "Servicios");
    assert.equal(
      resolveText("en", "onlyEs", { es: { onlyEs: "hola" }, en: {} }),
      "hola",
    );
    assert.equal(resolveText("en", "missing", { es: {}, en: {} }), "missing");
  });

  it("rellena los huecos del texto", () => {
    assert.equal(translate("es", "portAlt", { list: "8081, 8082" }), "alternativas: 8081, 8082");
    assert.equal(translate("en", "openLocal", { port: 8080 }), "Open localhost :8080");
    assert.equal(translate("es", "busyInProgress", { op: "setup" }), "ya hay una operación en curso (setup)");
    assert.equal(translate("es", "portAlt", {}), "alternativas: {list}");
  });
});
