use std::process::{Command, Output};

fn ktuner(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ktuner"))
        .args(arguments)
        .output()
        .expect("run ktuner")
}

/// Number of entries in `check`'s output that a real `tune` run would leave
/// out: the ones this host cannot write plus the knob that is dangerous to
/// change at runtime.
fn entries_a_real_run_skips(recommendations: &[serde_json::Value]) -> usize {
    recommendations
        .iter()
        .filter(|rec| rec["writable"] != true || rec["param"] == "vm.nr_hugepages")
        .count()
}

#[test]
fn dry_run_previews_the_plan_when_the_caller_cannot_write_sysctl() {
    // `tune --dry-run` is documented as a preview that needs no root and writes
    // nothing (README "preview, no changes"; the user guide's permission table
    // lists it as "Previews changes, writes nothing", Root: No). On a host whose
    // /proc/sys the caller cannot write — a plain non-root user, or a container
    // with read-only sysctl — the applicability filter used to empty the plan
    // *before* the dry-run branch, so the preview printed
    // `{"status": "optimal", "applied": 0}`: nothing previewed and a claim that
    // a host `check` had just listed recommendations for needed no tuning.
    let check = ktuner(&["check"]);
    let evaluation: serde_json::Value =
        serde_json::from_slice(&check.stdout).expect("check must print JSON on stdout");
    let findings = evaluation["recommendations"]
        .as_array()
        .expect("check must print a recommendations array");
    if findings.is_empty() {
        eprintln!("skip: this host has no recommendations to preview");
        return;
    }

    let plan = ktuner(&["tune", "--dry-run"]);
    assert_eq!(
        plan.status.code(),
        Some(0),
        "tune --dry-run must not fail: {}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let output: serde_json::Value =
        serde_json::from_slice(&plan.stdout).expect("tune --dry-run must print JSON on stdout");
    assert_ne!(
        output["status"],
        "optimal",
        "tune --dry-run called the host optimal while check reported {} findings: {output}",
        findings.len()
    );
    let previewed = output["would_apply"]
        .as_array()
        .unwrap_or_else(|| panic!("tune --dry-run must preview the plan: {output}"));
    assert_eq!(
        previewed.len(),
        findings.len(),
        "the preview must list every finding check reported: {output}"
    );
    // The preview also has to say how much of it this host cannot take: the
    // dry run itself writes nothing, so a plan is not a promise that the
    // entries are writable here.
    assert_eq!(
        output["skipped"].as_u64(),
        Some(entries_a_real_run_skips(findings) as u64),
        "skipped count must match the entries a real run would leave out: {output}"
    );
}
