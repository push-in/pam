pub struct DashboardApplication {
    pub name: String,
    pub kind_label: &'static str,
    pub online: bool,
    pub workers: usize,
    pub rss_bytes: u64,
    pub tasks: u64,
    pub alert_label: &'static str,
    pub alert_class: &'static str,
    pub warning: bool,
    pub history_samples: usize,
    pub peak_rss_bytes: u64,
    pub rss_delta_bytes: Option<i128>,
}

pub fn render(applications: &[DashboardApplication]) -> String {
    let online_count = applications.iter().filter(|app| app.online).count();
    let warning_count = applications.iter().filter(|app| app.warning).count();
    let total_rss = applications
        .iter()
        .fold(0_u64, |total, app| total.saturating_add(app.rss_bytes));
    let total_tasks = applications
        .iter()
        .fold(0_u64, |total, app| total.saturating_add(app.tasks));
    let summary = if warning_count > 0 {
        format!(
            "{warning_count} resource warning{}",
            if warning_count == 1 { "" } else { "s" }
        )
    } else if online_count == 0 {
        "No applications online".to_owned()
    } else {
        "All observed applications healthy".to_owned()
    };
    let mut rows = String::new();
    for app in applications {
        let state_label = if app.online { "Online" } else { "Stopped" };
        let state_class = if app.online { "online" } else { "stopped" };
        rows.push_str(&format!(
            "<tr><th scope=\"row\"><span class=\"app-name\">{}</span><span class=\"app-kind\">{}</span></th><td><span class=\"state {state_class}\">{state_label}</span></td><td class=\"number\">{}</td><td class=\"number\">{}</td><td class=\"number\">{}</td><td><span class=\"alert {}\">{}</span></td><td class=\"number\">{}</td><td class=\"number\">{}</td><td>{}</td></tr>",
            escape_html(&app.name),
            app.kind_label,
            app.workers,
            format_bytes(app.rss_bytes),
            app.tasks,
            app.alert_class,
            app.alert_label,
            app.history_samples,
            format_bytes(app.peak_rss_bytes),
            format_delta(app.rss_delta_bytes),
        ));
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=\"9\" class=\"empty\">No managed applications yet. Start one with <code>pam up … --name api</code>.</td></tr>");
    }
    let mut html = String::with_capacity(DOCUMENT_START.len() + rows.len() + 1_024);
    html.push_str(DOCUMENT_START);
    html.push_str(&escape_html(&summary));
    html.push_str(SUMMARY_MIDDLE);
    html.push_str(&applications.len().to_string());
    html.push_str(SUMMARY_ONLINE);
    html.push_str(&online_count.to_string());
    html.push_str(SUMMARY_MEMORY);
    html.push_str(&format_bytes(total_rss));
    html.push_str(SUMMARY_TASKS);
    html.push_str(&total_tasks.to_string());
    html.push_str(TABLE_START);
    html.push_str(&rows);
    html.push_str(DOCUMENT_END);
    html
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_delta(delta: Option<i128>) -> String {
    match delta {
        Some(delta) if delta > 0 => format!("Up {}", format_bytes(delta as u64)),
        Some(delta) if delta < 0 => format!("Down {}", format_bytes(delta.unsigned_abs() as u64)),
        Some(_) => "Stable".to_owned(),
        None => "Collecting".to_owned(),
    }
}

const DOCUMENT_START: &str = r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="dark light"><title>PAM manager snapshot</title><style>
:root{--ink:#eaf2ef;--muted:#9aa9a4;--surface:#111815;--panel:#17211d;--line:#304039;--signal:#72f2b0;--warn:#ffd166;--stop:#ff8f8f;--paper:#08100d}*{box-sizing:border-box}html{background:var(--paper);color:var(--ink);font:16px/1.5 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif}body{margin:0}.skip{position:absolute;left:1rem;top:-5rem;background:var(--signal);color:#062016;padding:.75rem 1rem;z-index:2}.skip:focus{top:1rem}header,main,footer{width:min(1180px,calc(100% - 2rem));margin-inline:auto}header{padding:clamp(2.5rem,8vw,6rem) 0 2rem;border-bottom:1px solid var(--line)}.eyebrow,.app-kind{color:var(--muted);font:700 .72rem/1.2 ui-monospace,SFMono-Regular,Consolas,monospace;letter-spacing:.12em;text-transform:uppercase}h1{max-width:850px;margin:.5rem 0 1rem;font-size:clamp(2.3rem,7vw,5.7rem);line-height:.9;letter-spacing:-.065em}.lede{max-width:65ch;color:var(--muted);font-size:1.05rem}.rail{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:1px;margin:2rem 0;background:var(--line);border:1px solid var(--line)}.metric{min-height:8rem;padding:1.25rem;background:var(--panel)}.metric strong{display:block;margin-top:.45rem;font:700 clamp(1.55rem,4vw,2.5rem)/1 ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums}.metric span{color:var(--muted);font-size:.8rem}section{margin:2rem 0 4rem}h2{font-size:1rem;letter-spacing:.08em;text-transform:uppercase}.table-wrap{overflow-x:auto;border:1px solid var(--line);background:var(--surface)}table{width:100%;border-collapse:collapse;min-width:1100px}caption{padding:1rem;text-align:left;color:var(--muted)}th,td{padding:1rem;border-top:1px solid var(--line);text-align:left}thead th{border-top:0;color:var(--muted);font-size:.75rem;text-transform:uppercase;letter-spacing:.08em}.app-name{display:block;font-weight:750}.app-kind{display:block;margin-top:.2rem}.number{font-family:ui-monospace,SFMono-Regular,Consolas,monospace;font-variant-numeric:tabular-nums}.state,.alert{display:inline-flex;align-items:center;gap:.45rem;font-weight:700}.state::before,.alert::before{content:"";width:.62rem;height:.62rem;border:2px solid currentColor;border-radius:50%}.online,.healthy{color:var(--signal)}.stopped{color:var(--stop)}.warning{color:var(--warn)}.unavailable{color:var(--muted)}.empty{padding:3rem;text-align:center;color:var(--muted)}code{color:var(--signal);font-family:ui-monospace,SFMono-Regular,Consolas,monospace}footer{padding:1.25rem 0 3rem;border-top:1px solid var(--line);color:var(--muted);font-size:.85rem}@media(max-width:720px){.rail{grid-template-columns:1fr 1fr}h1{letter-spacing:-.045em}}@media(prefers-color-scheme:light){:root{--ink:#15211c;--muted:#53635c;--surface:#fff;--panel:#eef5f1;--line:#bccbc4;--signal:#087443;--warn:#8a5700;--stop:#a52c32;--paper:#f7faf8}}@media(prefers-contrast:more){:root{--line:currentColor}}@media(prefers-reduced-motion:reduce){*{scroll-behavior:auto!important}}</style></head><body><a class="skip" href="#applications">Skip to applications</a><header><p class="eyebrow">PAM / manager flight recorder</p><h1>"##;
const SUMMARY_MIDDLE: &str = r##"</h1><p class="lede">A private, read-only snapshot of process health and capacity. It contains process metadata, but excludes commands, paths, environment values, network data and log contents.</p></header><main><section class="rail" aria-label="Manager summary"><div class="metric"><span>Managed</span><strong>"##;
const SUMMARY_ONLINE: &str = r##"</strong></div><div class="metric"><span>Online</span><strong>"##;
const SUMMARY_MEMORY: &str =
    r##"</strong></div><div class="metric"><span>Resident memory</span><strong>"##;
const SUMMARY_TASKS: &str = r##"</strong></div><div class="metric"><span>Tasks</span><strong>"##;
const TABLE_START: &str = r##"</strong></div></section><section id="applications" aria-labelledby="applications-title"><h2 id="applications-title">Applications</h2><div class="table-wrap" tabindex="0" role="region" aria-label="Managed applications, horizontally scrollable"><table><caption>Current process state with bounded one-minute history. Status and trends are written as text and never communicated by color alone.</caption><thead><tr><th scope="col">Application</th><th scope="col">State</th><th scope="col">Workers</th><th scope="col">RSS</th><th scope="col">Tasks</th><th scope="col">Resource signal</th><th scope="col">Samples</th><th scope="col">Peak RSS</th><th scope="col">RSS trend</th></tr></thead><tbody>"##;
const DOCUMENT_END: &str = r##"</tbody></table></div></section></main><footer>Generated locally by <code>pam dashboard</code>. Refresh by creating a new snapshot; existing files are never overwritten.</footer></body></html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_is_accessible_bounded_and_escapes_application_names() {
        let html = render(&[DashboardApplication {
            name: "api<&\"'".to_owned(),
            kind_label: "PAM Runtime",
            online: true,
            workers: 2,
            rss_bytes: 1_572_864,
            tasks: 4,
            alert_label: "Healthy",
            alert_class: "healthy",
            warning: false,
            history_samples: 2,
            peak_rss_bytes: 1_572_864,
            rss_delta_bytes: Some(524_288),
        }]);
        assert!(html.len() < 64 * 1024);
        assert!(html.contains("api&lt;&amp;&quot;&#39;"));
        assert!(html.contains("All observed applications healthy"));
        assert!(html.contains("1.5 MiB"));
        assert!(html.contains("href=\"#applications\""));
        assert!(html.contains("prefers-color-scheme:light"));
        assert!(html.contains("prefers-contrast:more"));
        assert!(html.contains("Status and trends are written as text"));
        assert!(html.contains("Up 512.0 KiB"));
        assert!(!html.contains("<script"));
    }
}
