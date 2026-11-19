use screencast_ui::{gui_assets, player_assets};
use std::process::Command;

fn screencast_bin() -> &'static str {
    env!("CARGO_BIN_EXE_screencast")
}

fn clean_cmd() -> Command {
    let mut cmd = Command::new(screencast_bin());
    cmd.env_remove("SCREENCAST_LANG");
    cmd.env_remove("LANG");
    cmd.env_remove("LC_ALL");
    cmd.env("XDG_CONFIG_HOME", "/nonexistent-screencast-test-dir");
    cmd
}

#[test]
fn cli_help_is_english_first_and_shows_lang_flag() {
    let out = clean_cmd().arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Cast your Linux desktop to a TV live"));
    assert!(stdout.contains("--lang <en|tr>"));
}

#[test]
fn cli_defaults_to_english_and_switches_with_flag_env_and_config() {
    // 1. Default (no flag, no env, no config) -> English
    let out_en = clean_cmd().args(["doctor", "--quality"]).output().unwrap();
    assert!(out_en.status.success());
    let s_en = String::from_utf8_lossy(&out_en.stdout);
    assert!(s_en.contains("environment check"), "stdout={s_en}");
    assert!(s_en.contains("== Session =="), "stdout={s_en}");

    // 2. --lang tr -> Turkish
    let out_tr = clean_cmd()
        .args(["--lang", "tr", "doctor", "--quality"])
        .output()
        .unwrap();
    assert!(out_tr.status.success());
    let s_tr = String::from_utf8_lossy(&out_tr.stdout);
    assert!(s_tr.contains("ortam kontrolü"), "stdout={s_tr}");
    assert!(s_tr.contains("== Oturum =="), "stdout={s_tr}");

    // 3. SCREENCAST_LANG=tr overrides LANG=en_US.UTF-8
    let out_env = clean_cmd()
        .env("LANG", "en_US.UTF-8")
        .env("SCREENCAST_LANG", "tr")
        .args(["doctor", "--quality"])
        .output()
        .unwrap();
    assert!(out_env.status.success());
    assert!(String::from_utf8_lossy(&out_env.stdout).contains("== Oturum =="));

    // 4. Isolated XDG_CONFIG_HOME with config.toml language = "tr"
    let tmp = std::env::temp_dir().join(format!("screencast_i18n_test_{}", std::process::id()));
    let cfg_dir = tmp.join("screencast");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    std::fs::write(cfg_dir.join("config.toml"), "language = \"tr\"\n").unwrap();

    let out_cfg = Command::new(screencast_bin())
        .env_remove("SCREENCAST_LANG")
        .env("LANG", "en_US.UTF-8")
        .env("XDG_CONFIG_HOME", &tmp)
        .args(["doctor", "--quality"])
        .output()
        .unwrap();
    assert!(out_cfg.status.success());
    assert!(
        String::from_utf8_lossy(&out_cfg.stdout).contains("== Oturum =="),
        "expected config language=tr to take effect over LANG"
    );

    // 5. --lang en overrides config.toml language = "tr" and SCREENCAST_LANG=tr
    let out_override = Command::new(screencast_bin())
        .env("SCREENCAST_LANG", "tr")
        .env("XDG_CONFIG_HOME", &tmp)
        .args(["--lang", "en", "doctor", "--quality"])
        .output()
        .unwrap();
    assert!(out_override.status.success());
    assert!(String::from_utf8_lossy(&out_override.stdout).contains("== Session =="));

    // 6. config init creates English default config
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let out_init = Command::new(screencast_bin())
        .env_remove("SCREENCAST_LANG")
        .env("XDG_CONFIG_HOME", &tmp)
        .args(["config", "init"])
        .output()
        .unwrap();
    assert!(out_init.status.success());
    let written = std::fs::read_to_string(cfg_dir.join("config.toml")).unwrap();
    assert!(written.contains("language = \"en\""));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn cli_rejects_invalid_lang_flag() {
    let out = clean_cmd()
        .args(["--lang", "de", "doctor"])
        .output()
        .unwrap();
    assert!(!out.status.success());

    let out_tr = clean_cmd()
        .env("SCREENCAST_LANG", "tr")
        .args(["--lang", "de", "doctor"])
        .output()
        .unwrap();
    assert!(!out_tr.status.success());
    let stderr_tr = String::from_utf8_lossy(&out_tr.stderr);
    assert!(
        stderr_tr.contains("geçerli diller"),
        "expected localized invalid-lang error in tr, got: {stderr_tr}"
    );
}

#[test]
fn cli_selftest_succeeds() {
    let out = clean_cmd().arg("selftest").output().unwrap();
    assert!(
        out.status.success(),
        "selftest stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("SELFTEST OK"));
}

#[test]
fn web_player_and_gui_assets_are_english_first_with_tr_catalogs() {
    let player = player_assets();
    assert!(player.index_html.contains("<html lang=\"en\">"));
    assert!(player.index_html.contains("Live Stream"));
    assert!(player.index_html.contains("/i18n.js"));
    assert!(player.index_html.contains("/i18n_tr.js"));
    assert!(player.i18n_js.contains("CONNECTING"));
    assert!(player.i18n_tr_js.contains("BAĞLANILIYOR"));

    let gui = gui_assets();
    let helper = gui
        .iter()
        .find(|(name, _)| *name == "screencast_gui_i18n.py")
        .unwrap()
        .1;
    let tr_cat = gui
        .iter()
        .find(|(name, _)| *name == "screencast_gui_i18n_tr.py")
        .unwrap()
        .1;
    assert!(helper.contains("return \"en\""));
    assert!(tr_cat.contains("TR_CATALOG"));
}

#[test]
fn core_text_keys_have_full_en_tr_coverage_and_matching_placeholders() {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let i18n_dir = root.join("crates/screencast-core/src/i18n");
    let keys_src = std::fs::read_to_string(i18n_dir.join("keys.rs")).unwrap();
    let start = keys_src.find("pub enum TextKey {").unwrap();
    let end = keys_src[start..].find('}').unwrap() + start;
    let keys: Vec<String> = keys_src[start..end]
        .lines()
        .skip(1)
        .map(|l| l.trim().trim_end_matches(',').to_string())
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    assert!(!keys.is_empty());

    fn load_catalog(dir: &Path) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for fname in ["core.rs", "sinks.rs", "cli.rs"] {
            let src = std::fs::read_to_string(dir.join(fname)).unwrap();
            let mut rest = src.as_str();
            while let Some(pos) = rest.find("TextKey::") {
                rest = &rest[pos + "TextKey::".len()..];
                let key_end = rest
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                let key = &rest[..key_end];
                rest = &rest[key_end..];
                let trimmed = rest.trim_start();
                if let Some(after_arrow) = trimmed.strip_prefix("=>") {
                    let body = after_arrow.trim_start();
                    let body = body.strip_prefix('{').map(str::trim_start).unwrap_or(body);
                    if let Some(after_q) = body.strip_prefix('"') {
                        let mut val = String::new();
                        let mut chars = after_q.chars();
                        while let Some(ch) = chars.next() {
                            if ch == '\\' {
                                if let Some(esc) = chars.next() {
                                    val.push(esc);
                                }
                            } else if ch == '"' {
                                break;
                            } else {
                                val.push(ch);
                            }
                        }
                        map.insert(key.to_string(), val);
                    }
                }
            }
        }
        map
    }

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        let mut rest = s;
        while let Some(open) = rest.find('{') {
            rest = &rest[open + 1..];
            if let Some(close) = rest.find('}') {
                set.insert(rest[..close].to_string());
                rest = &rest[close + 1..];
            } else {
                break;
            }
        }
        set
    }

    let en = load_catalog(&i18n_dir.join("en"));
    let tr = load_catalog(&i18n_dir.join("tr"));
    for key in &keys {
        let en_val = en
            .get(key)
            .unwrap_or_else(|| panic!("missing EN entry for TextKey::{key}"));
        let tr_val = tr
            .get(key)
            .unwrap_or_else(|| panic!("missing TR entry for TextKey::{key}"));
        assert!(
            !en_val.is_empty() && !tr_val.is_empty(),
            "empty translation for TextKey::{key}"
        );
        assert_eq!(
            placeholders(en_val),
            placeholders(tr_val),
            "placeholder mismatch for TextKey::{key}"
        );
    }
}

#[test]
fn gui_i18n_helper_error_classification_and_config_precedence() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let gui_dir = root.join("assets/gui");
    let script = r#"
import os, sys, tempfile
sys.path.insert(0, sys.argv[1])
from screencast_gui_i18n import EN_CATALOG, GuiI18n, detect_initial_lang
from screencast_gui_i18n_tr import TR_CATALOG

assert set(EN_CATALOG.keys()) == set(TR_CATALOG.keys())

en = GuiI18n("en")
assert en.friendly("No target device found. Scan the network with `screencast discover`.") == EN_CATALOG["err_no_sink"]
assert en.friendly("Target device refused connection: RTSP 403") == EN_CATALOG["err_unreachable"]
assert en.friendly("Screen sharing permission denied (User denied screen sharing during Start)") == EN_CATALOG["err_portal"]
assert en.friendly("Wi-Fi Direct P2P connection refused (aa:bb): FAIL") == EN_CATALOG["err_p2p"]

tr = GuiI18n("tr")
assert tr.friendly("Hedef cihaz bulunamadı. `screencast discover` ile ağı tara.") == TR_CATALOG["err_no_sink"]
assert tr.friendly("Hedef cihaz bağlantıyı reddetti: RTSP 403") == TR_CATALOG["err_unreachable"]
assert tr.friendly("Ekran paylaşım izni verilmedi (iptal)") == TR_CATALOG["err_portal"]
assert tr.friendly("Wi-Fi Direct P2P bağlantısı reddedildi (aa:bb): FAIL") == TR_CATALOG["err_p2p"]

with tempfile.TemporaryDirectory() as td:
    sc = os.path.join(td, "screencast")
    os.makedirs(sc)
    with open(os.path.join(sc, "config.toml"), "w", encoding="utf-8") as f:
        f.write('schema_version = 1\nlanguage = "tr"\n')
    assert detect_initial_lang([], {"XDG_CONFIG_HOME": td, "LANG": "en_US.UTF-8"}) == "tr"
    assert detect_initial_lang(["--lang", "en"], {"XDG_CONFIG_HOME": td, "SCREENCAST_LANG": "tr"}) == "en"
"#;
    let out = Command::new("python3")
        .args(["-B", "-c", script, gui_dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "python gui i18n check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
