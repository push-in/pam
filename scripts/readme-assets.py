#!/usr/bin/env python3
"""Render the README benchmark chart and architecture diagram (light and dark SVG).

Benchmark numbers come from recorded evidence.

The numbers below are copied verbatim from report.json and resources.json of the
`pam-evidence-suite-1-91d3ba8…` artifact produced by the Performance evidence
workflow (run 41623674573, 2026-11-03, PAM 2.2.9). Update DATA and SOURCE
together when a newer evidence run replaces it, then run:

    python3 scripts/readme-assets.py
"""

from __future__ import annotations

from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "docs" / "assets" / "readme"
SOURCE = "PAM 2.2.9 · hosted Performance evidence run 41623674573 · 1 worker · 128 connections · median of 5 × 15 s rounds"

CAVEAT = "PAM ran PHP 8.5.12; FrankenPHP and OpenSwoole ran PHP 8.4.24 (OPcache + JIT on all three). Not a certified result: see the README."

RUNTIMES = ["PAM", "FrankenPHP", "OpenSwoole"]
SCENARIOS = ["Minimal JSON", "Blade view", "SQLite query", "Large JSON"]
# scenario -> runtime -> (median requests/s, median p99 µs)
DATA = {
    "Minimal JSON": {"PAM": (1665.76, 81200), "FrankenPHP": (1572.69, 94780), "OpenSwoole": (1222.35, 113580)},
    "Blade view": {"PAM": (1266.81, 105820), "FrankenPHP": (1171.61, 121550), "OpenSwoole": (939.69, 146240)},
    "SQLite query": {"PAM": (1282.73, 103850), "FrankenPHP": (1184.27, 125150), "OpenSwoole": (970.64, 143520)},
    "Large JSON": {"PAM": (1550.61, 87080), "FrankenPHP": (1425.14, 103920), "OpenSwoole": (1138.59, 122940)},
}

THEMES = {
    "light": {
        "series": ["#2a78d6", "#eb6834", "#1baf7a"],
        "text": "#1f2328", "muted": "#59636e", "grid": "#d1d9e0", "baseline": "#818b98",
    },
    "dark": {
        "series": ["#3987e5", "#d95926", "#199e70"],
        "text": "#f0f6fc", "muted": "#9198a1", "grid": "#3d444d", "baseline": "#656c76",
    },
}

FONT = "-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif"
BAR, BAR_GAP, GROUP_GAP = 13, 2, 18


def bar(x: float, y: float, width: float, color: str) -> str:
    # Rounded data end, square baseline end.
    r = min(4, width / 2)
    return (f'<path d="M{x:.1f},{y:.1f}h{width - r:.1f}a{r},{r} 0 0 1 {r},{r}'
            f'v{BAR - 2 * r:.1f}a{r},{r} 0 0 1 -{r},{r}h-{width - r:.1f}z" fill="{color}"/>')


def panel(theme: dict, x0: float, y0: float, width: float, title: str, subtitle: str,
          value, maximum: float, ticks: list[float], fmt, tick_fmt) -> list[str]:
    out = [
        f'<text x="{x0}" y="{y0}" font-size="14" font-weight="600" fill="{theme["text"]}">{title}</text>',
        f'<text x="{x0}" y="{y0 + 18}" font-size="12" fill="{theme["muted"]}">{subtitle}</text>',
    ]
    label_w = 96
    plot_x = x0 + label_w
    plot_w = width - label_w - 52
    top = y0 + 36
    group_h = 3 * BAR + 2 * BAR_GAP
    bottom = top + len(SCENARIOS) * (group_h + GROUP_GAP) - GROUP_GAP
    for tick in ticks:
        tx = plot_x + plot_w * tick / maximum
        out.append(f'<line x1="{tx:.1f}" y1="{top - 4}" x2="{tx:.1f}" y2="{bottom + 4}" stroke="{theme["grid"]}" stroke-width="1"/>')
        out.append(f'<text x="{tx:.1f}" y="{bottom + 18}" font-size="11" text-anchor="middle" fill="{theme["muted"]}">{tick_fmt(tick)}</text>')
    out.append(f'<line x1="{plot_x}" y1="{top - 4}" x2="{plot_x}" y2="{bottom + 4}" stroke="{theme["baseline"]}" stroke-width="1"/>')
    for gi, scenario in enumerate(SCENARIOS):
        gy = top + gi * (group_h + GROUP_GAP)
        out.append(f'<text x="{plot_x - 10}" y="{gy + group_h / 2 + 4:.1f}" font-size="12" text-anchor="end" fill="{theme["text"]}">{scenario}</text>')
        for ri, runtime in enumerate(RUNTIMES):
            v = value(DATA[scenario][runtime])
            by = gy + ri * (BAR + BAR_GAP)
            w = plot_w * v / maximum
            out.append(bar(plot_x + 1, by, w, theme["series"][ri]))
            weight = ' font-weight="600"' if runtime == "PAM" else ""
            out.append(f'<text x="{plot_x + w + 6:.1f}" y="{by + BAR - 2.5:.1f}" font-size="11"{weight} fill="{theme["text"]}">{fmt(v)}</text>')
    return out


def render(mode: str) -> str:
    t = THEMES[mode]
    width, height = 920, 368
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" '
        f'font-family="{FONT}" role="img" aria-label="Laravel throughput and p99 latency for PAM, FrankenPHP and OpenSwoole, one worker each. {SOURCE}.">',
    ]
    # Legend
    lx = 0
    for i, runtime in enumerate(RUNTIMES):
        parts.append(f'<rect x="{lx}" y="2" width="12" height="12" rx="3" fill="{t["series"][i]}"/>')
        parts.append(f'<text x="{lx + 18}" y="12.5" font-size="12.5" fill="{t["text"]}">{runtime}</text>')
        lx += 18 + len(runtime) * 7.6 + 22
    parts += panel(t, 0, 44, 460, "Throughput", "requests per second · higher is better",
                   lambda d: d[0], 2000, [0, 500, 1000, 1500, 2000],
                   lambda v: f"{v:,.0f}", lambda v: f"{v:,.0f}")
    parts += panel(t, 470, 44, 450, "p99 latency", "milliseconds · lower is better",
                   lambda d: d[1] / 1000, 160, [0, 40, 80, 120, 160],
                   lambda v: f"{v:.1f}", lambda v: f"{v:.0f}")
    parts.append(f'<text x="0" y="{height - 22}" font-size="11" fill="{t["muted"]}">{SOURCE}</text>')
    parts.append(f'<text x="0" y="{height - 6}" font-size="11" fill="{t["muted"]}">{CAVEAT}</text>')
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


DIAGRAM = {
    "light": {"text": "#1f2328", "muted": "#59636e", "box": "#f6f8fa", "stroke": "#d1d9e0",
              "accent": "#2a78d6", "accent_fill": "#ddf4ff", "line": "#818b98"},
    "dark": {"text": "#f0f6fc", "muted": "#9198a1", "box": "#151b23", "stroke": "#3d444d",
             "accent": "#4493f8", "accent_fill": "#0d2846", "line": "#656c76"},
}


def architecture(mode: str) -> str:
    c = DIAGRAM[mode]
    o: list[str] = []

    def box(x, y, w, h, title, lines=(), accent=False, dashed=False, title_size=13):
        fill = "none" if dashed else (c["accent_fill"] if accent else c["box"])
        stroke = c["accent"] if accent else c["stroke"]
        dash = ' stroke-dasharray="5 4"' if dashed else ""
        o.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="{fill}" stroke="{stroke}" stroke-width="1.2"{dash}/>')
        o.append(f'<text x="{x + 14}" y="{y + 22}" font-size="{title_size}" font-weight="600" fill="{c["text"]}">{title}</text>')
        for i, line in enumerate(lines):
            o.append(f'<text x="{x + 14}" y="{y + 42 + i * 17}" font-size="11.5" fill="{c["muted"]}">{line}</text>')

    def arrow(x1, y1, x2, y2, label="", lx=None, ly=None, accent=False, anchor="middle"):
        color = c["accent"] if accent else c["line"]
        marker = "a" if accent else "m"
        o.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="1.6" marker-end="url(#{marker})"/>')
        if label:
            lx = (x1 + x2) / 2 if lx is None else lx
            ly = (y1 + y2) / 2 - 6 if ly is None else ly
            o.append(f'<text x="{lx}" y="{ly}" font-size="11" text-anchor="{anchor}" fill="{color if accent else c["muted"]}">{label}</text>')

    W, H = 960, 560
    o.append(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}" role="img" '
             'aria-label="PAM architecture: a supervised master runs worker processes; in each worker Rust and Tokio own the network and I/O while the Zend Engine keeps the Composer application in memory and runs each request in a Fiber; Composer packages add HTTP, Laravel, Native and Desktop.">')
    o.append('<defs>'
             f'<marker id="m" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0L10,5L0,10z" fill="{c["line"]}"/></marker>'
             f'<marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0L10,5L0,10z" fill="{c["accent"]}"/></marker>'
             '</defs>')

    # Control plane
    box(40, 20, 250, 74, "pamd · process manager", ["pam up · reload · scale · logs", "canary traffic · health checks"])
    box(360, 20, 560, 74, "Master control plane", ["crash recovery · watchdog · graceful drain · generational reload", "/live · /ready · /metrics served outside PHP"])
    arrow(290, 57, 358, 57, "manages")
    arrow(640, 94, 640, 130, "supervises N workers · SO_REUSEPORT", lx=652, ly=116, anchor="start")

    # Worker
    box(260, 134, 660, 206, "Worker process", dashed=True)
    box(30, 200, 140, 74, "Clients", ["browsers · apps · APIs"])
    arrow(170, 237, 278, 237, "TCP · TLS · QUIC", ly=228)
    box(280, 168, 270, 152, "Rust · Tokio", ["HTTP/1.1 · HTTP/2 · HTTP/3", "WebSocket · Rustls TLS", "timers · sockets · async I/O", "limits · backpressure · metrics"], accent=True)
    box(670, 168, 230, 152, "Zend Engine · Embed SAPI", ["application booted once", "Composer autoloaded once"])
    for i in range(3):
        fx = 684 + i * 70
        o.append(f'<rect x="{fx}" y="262" width="62" height="40" rx="7" fill="{c["accent_fill"]}" stroke="{c["accent"]}" stroke-width="1"/>')
        o.append(f'<text x="{fx + 31}" y="279" font-size="11" text-anchor="middle" font-weight="600" fill="{c["text"]}">Fiber</text>')
        o.append(f'<text x="{fx + 31}" y="294" font-size="10" text-anchor="middle" fill="{c["muted"]}">request {i + 1 if i < 2 else "N"}</text>')
    arrow(550, 214, 668, 214, "dispatch request", accent=True, ly=206)
    arrow(668, 272, 552, 272, "suspend on I/O", accent=True, ly=264)
    o.append(f'<text x="610" y="290" font-size="11" text-anchor="middle" fill="{c["accent"]}">resume with result</text>')

    # Composer packages
    chips = [("pushinbr/pam-http", 40), ("pam-laravel · pam-octane", 250), ("pushinbr/pam-native", 490), ("pushinbr/pam-desktop", 710)]
    for label, x in chips:
        o.append(f'<rect x="{x}" y="400" width="{200 if x < 700 else 210}" height="36" rx="8" fill="{c["box"]}" stroke="{c["stroke"]}" stroke-width="1.2"/>')
        o.append(f'<text x="{x + (100 if x < 700 else 105)}" y="423" font-size="12" text-anchor="middle" fill="{c["text"]}">{label}</text>')
    o.append(f'<path d="M140,398 V374 H785 M350,398 V374 M785,374 V324" fill="none" stroke="{c["line"]}" stroke-width="1.6" marker-end="url(#m)"/>')
    o.append(f'<text x="460" y="366" font-size="11" text-anchor="middle" fill="{c["muted"]}">server packages load once into the warm process</text>')

    # Rendering surfaces
    box(490, 470, 200, 74, "Android Views · UIKit", ["PHP tree → Rust diff + Yoga", "→ UI-thread commit"])
    box(710, 470, 210, 74, "GTK 4 · AppKit · Win32", ["PAM Desktop 2.0 preview", "no web view in the path"])
    arrow(590, 436, 590, 468, "renders", lx=600, ly=456, anchor="start")
    arrow(815, 436, 815, 468, "renders", lx=825, ly=456, anchor="start")
    box(40, 470, 410, 74, "Your PHP stays PHP", ["PSR-7/15/17 · Laravel 12/13 · Pest · PHPUnit · Eloquent", "no fork, no new language, no second package manager"])

    o.append("</svg>")
    return "\n".join(o) + "\n"


def hero(mode: str) -> str:
    c = DIAGRAM[mode]
    W, H = 960, 250
    o = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="{FONT}" role="img" '
         'aria-label="PAM, PHP Always in Memory: one persistent PHP runtime for servers, Laravel, Android and iOS, and desktop.">']
    o.append(f'<rect x="1" y="1" width="{W - 2}" height="{H - 2}" rx="18" fill="{c["box"]}" stroke="{c["stroke"]}"/>')
    # Wordmark: the process ring with the application held inside it.
    o.append(f'<circle cx="96" cy="96" r="44" fill="none" stroke="{c["accent"]}" stroke-width="10"/>')
    o.append(f'<circle cx="96" cy="96" r="16" fill="{c["accent"]}"/>')
    o.append(f'<text x="164" y="92" font-size="56" font-weight="800" letter-spacing="2" fill="{c["text"]}">PAM</text>')
    o.append(f'<text x="168" y="126" font-size="18" font-weight="600" fill="{c["accent"]}">PHP, Always in Memory</text>')
    o.append(f'<text x="480" y="78" font-size="20" font-weight="600" fill="{c["text"]}">Boot PHP once. Keep it warm.</text>')
    o.append(f'<text x="480" y="106" font-size="14" fill="{c["muted"]}">A Rust + Tokio runtime that keeps the Zend Engine and</text>')
    o.append(f'<text x="480" y="126" font-size="14" fill="{c["muted"]}">your Composer app warm, then ships it to every surface.</text>')
    pillars = [("Server", "HTTP/1.1 · 2 · 3 · WebSocket"), ("Laravel", "warm kernel · Octane bridge"),
               ("Android · iOS", "native views, no web view"), ("Desktop", "Linux · macOS · Windows")]
    for i, (title, sub) in enumerate(pillars):
        x = 40 + i * 224
        o.append(f'<rect x="{x}" y="164" width="208" height="58" rx="10" fill="none" stroke="{c["stroke"]}"/>')
        o.append(f'<text x="{x + 16}" y="188" font-size="14" font-weight="700" fill="{c["text"]}">{title}</text>')
        o.append(f'<text x="{x + 16}" y="208" font-size="12" fill="{c["muted"]}">{sub}</text>')
    o.append("</svg>")
    return "\n".join(o) + "\n"


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for mode in THEMES:
        (OUT / f"benchmark-laravel-{mode}.svg").write_text(render(mode), encoding="utf-8")
        (OUT / f"architecture-{mode}.svg").write_text(architecture(mode), encoding="utf-8")
        (OUT / f"hero-{mode}.svg").write_text(hero(mode), encoding="utf-8")
    print(f"wrote {OUT}/{{hero,benchmark-laravel,architecture}}-{{light,dark}}.svg")


if __name__ == "__main__":
    main()
