// Mide RAM y CPU de la app nativa con agentes simulados (ADR 0006, ficha del Hito 2).
//
// Uso: node scripts/measure.mjs [--sim=20:42:freeze|live] [--warmup=15] [--seconds=10] [--bin=target/release/kiti-app]
//
// Cuenta el proceso de la app y los procesos XPC de WebKit (WebContent, GPU, Networking) que
// aparecen al lanzarla. RAM: `footprint` (memoria física atribuida, lo que muestra el Monitor de
// Actividad) y RSS de `ps`. CPU: tiempo de CPU consumido dentro de la ventana de muestreo
// (`ps -o time`), no `%cpu`, que en macOS es una media que arrastra el pico del arranque.
import { execFileSync, spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const arg = (name, fallback) =>
  process.argv.find((a) => a.startsWith(`--${name}=`))?.split("=")[1] ?? fallback;

const sim = arg("sim", "20:42:freeze");
const warmup = Number(arg("warmup", "15"));
const seconds = Number(arg("seconds", "10"));
const bin = arg("bin", "target/release/kiti-app");

/** `[[dd-]hh:]mm:ss.cc` → segundos de CPU acumulados. */
function cpuSeconds(time) {
  const [days, rest] = time.includes("-") ? time.split("-") : ["0", time];
  const parts = rest.split(":").map(Number);
  while (parts.length < 3) parts.unshift(0);
  const [h, m, sec] = parts;
  return Number(days) * 86400 + h * 3600 + m * 60 + sec;
}

function processes() {
  const out = execFileSync("ps", ["-A", "-o", "pid=,rss=,time=,comm="], { encoding: "utf8" });
  return out
    .trim()
    .split("\n")
    .map((line) => {
      const [pid, rss, time, ...comm] = line.trim().split(/\s+/);
      return { pid: Number(pid), rssKb: Number(rss), cpuS: cpuSeconds(time), comm: comm.join(" ") };
    });
}

const isWebKit = (p) => p.comm.includes("com.apple.WebKit.");
const before = new Set(processes().filter(isWebKit).map((p) => p.pid));

// --sim=live mide el modo real (escaneo de procesos cada 2 s) en lugar del simulador.
const env = { ...process.env };
if (sim === "live") delete env.KITI_SIM;
else env.KITI_SIM = sim;
const app = spawn(bin, [], { env, stdio: "ignore" });
await sleep(warmup * 1000);

const ours = () =>
  processes().filter((p) => p.pid === app.pid || (isWebKit(p) && !before.has(p.pid)));

const samples = new Map();
const t0 = Date.now();
for (let i = 0; i <= seconds; i++) {
  for (const p of ours()) {
    const s = samples.get(p.pid) ?? { comm: p.comm, rssKb: [], cpuS: [] };
    s.rssKb.push(p.rssKb);
    s.cpuS.push(p.cpuS);
    samples.set(p.pid, s);
  }
  if (i < seconds) await sleep(1000);
}
const windowS = (Date.now() - t0) / 1000;

function footprintMb(pid) {
  try {
    const out = execFileSync("footprint", [String(pid)], { encoding: "utf8" });
    const m = out.match(/Footprint:\s+([\d.]+)\s+(KB|MB|GB)/);
    if (!m) return Number.NaN;
    const scale = { KB: 1 / 1024, MB: 1, GB: 1024 }[m[2]];
    return Number(m[1]) * scale;
  } catch {
    return Number.NaN;
  }
}

const avg = (xs) => xs.reduce((a, b) => a + b, 0) / xs.length;
const rows = [...samples.entries()].map(([pid, s]) => ({
  proceso: s.comm.split("/").pop(),
  pid,
  footprintMb: Number(footprintMb(pid).toFixed(1)),
  rssMb: Number((avg(s.rssKb) / 1024).toFixed(1)),
  cpuMedia: Number(((100 * (s.cpuS.at(-1) - s.cpuS[0])) / windowS).toFixed(2)),
}));
app.kill("SIGTERM");

console.table(rows);
const total = (k) => Number(rows.reduce((a, r) => a + (Number.isNaN(r[k]) ? 0 : r[k]), 0).toFixed(2));
console.log(
  JSON.stringify({
    sim,
    warmupS: warmup,
    sampleS: seconds,
    footprintMb: total("footprintMb"),
    rssMb: total("rssMb"),
    cpuPercent: total("cpuMedia"),
  }),
);
