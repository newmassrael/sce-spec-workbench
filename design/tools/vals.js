// Run a board's own script against a stub runtime, press things, and print what it would render.
//
//   node vals.js <board.dc.html> [action ...]            prints the values as JSON, for render.py
//   PROPS='{"policy":true}' node vals.js Connect.dc.html goLocal
//
// An action is the name of a function the script returns, or name=text for one that takes an input's
// value (the stub passes {target:{value:text}}). Actions run in order; the values are read after each.
// A function is never printed. A list of objects keeps its fields (functions removed), so a board that
// repeats with <sc-for> can be rendered. This is the same stub the tests use: it checks the logic of a
// board, not the canvas runtime.

const fs = require("fs");

function load(file, props) {
  const html = fs.readFileSync(file, "utf8");
  const m = html.match(/<script type="text\/x-dc"[^>]*>([\s\S]*?)<\/script>/);
  if (!m) throw new Error(file + ": no script");
  class DCLogic {
    constructor(p) { this.props = p; }
    setState(s) { this.state = { ...this.state, ...s }; }
  }
  const C = new Function("DCLogic", m[1] + "; return Component;")(DCLogic);
  const c = new C(props);
  return () => c.renderVals();
}

function plain(v) {
  if (typeof v === "function") return undefined;
  if (Array.isArray(v)) return v.map(plain);
  if (v && typeof v === "object") {
    const o = {};
    for (const [k, x] of Object.entries(v)) {
      const p = plain(x);
      if (p !== undefined) o[k] = p;
    }
    return o;
  }
  return v;
}

const [file, ...actions] = process.argv.slice(2);
const read = load(file, JSON.parse(process.env.PROPS || "{}"));
for (const a of actions) {
  const eq = a.indexOf("=");
  const name = eq < 0 ? a : a.slice(0, eq);
  const fn = read()[name];
  if (typeof fn !== "function") throw new Error("no action " + name);
  eq < 0 ? fn() : fn({ target: { value: a.slice(eq + 1) } });
}
process.stdout.write(JSON.stringify(plain(read())));
