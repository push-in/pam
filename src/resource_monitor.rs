use std::collections::BTreeMap;
use std::fs;

use serde::Serialize;

const MAX_PROCESSES: usize = 65_536;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSnapshot {
    pub rss_bytes: u64,
    pub tasks: u64,
    pub processes: u64,
    pub observed: bool,
}

pub fn process_tree(root_pid: u32) -> ResourceSnapshot {
    process_tree_at(root_pid, std::path::Path::new("/proc"))
}

fn process_tree_at(root_pid: u32, proc_root: &std::path::Path) -> ResourceSnapshot {
    let Ok(entries) = fs::read_dir(proc_root) else {
        return ResourceSnapshot::default();
    };
    let mut parents = BTreeMap::new();
    for entry in entries.flatten().take(MAX_PROCESSES) {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        if let Ok(stat) = fs::read_to_string(entry.path().join("stat"))
            && let Some(parent) = stat_parent(&stat)
        {
            parents.insert(pid, parent);
        }
    }
    let mut children = BTreeMap::<u32, Vec<u32>>::new();
    for (pid, parent) in parents {
        children.entry(parent).or_default().push(pid);
    }
    let mut selected = Vec::with_capacity(children.len().min(1024) + 1);
    let mut pending = vec![root_pid];
    while let Some(pid) = pending.pop() {
        selected.push(pid);
        if let Some(descendants) = children.get(&pid) {
            pending.extend(descendants);
        }
    }
    let mut snapshot = ResourceSnapshot::default();
    for pid in selected {
        if let Ok(status) = fs::read_to_string(proc_root.join(pid.to_string()).join("status")) {
            snapshot.observed = true;
            snapshot.processes += 1;
            snapshot.rss_bytes = snapshot
                .rss_bytes
                .saturating_add(status_value(&status, "VmRSS:").saturating_mul(1024));
            snapshot.tasks = snapshot
                .tasks
                .saturating_add(status_value(&status, "Threads:"));
        }
    }
    snapshot
}

fn stat_parent(stat: &str) -> Option<u32> {
    let suffix = stat.rsplit_once(") ")?.1;
    suffix.split_whitespace().nth(1)?.parse().ok()
}

fn status_value(status: &str, key: &str) -> u64 {
    status
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_parent_after_names_containing_spaces_and_parentheses() {
        assert_eq!(stat_parent("42 (php worker (blue)) S 7 42 42 0"), Some(7));
    }

    #[test]
    fn observes_the_current_linux_process() {
        let snapshot = process_tree(std::process::id());
        assert!(snapshot.observed);
        assert!(snapshot.processes >= 1);
        assert!(snapshot.tasks >= 1);
        assert!(snapshot.rss_bytes > 0);
    }
}
