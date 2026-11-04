//! Offline crash analysis.
//!
//! Every rule here is a plain-text check over the log. No network, no model, no
//! consent: this layer always runs, and it is what the launcher shows even when
//! the user never configures a provider.
//!
//! Messages are Polish because they are read by the person fixing the instance;
//! identifiers and technical terms stay in their original form so they can be
//! searched verbatim.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnosis {
    /// Stable rule id, so the UI can group or de-duplicate.
    pub id: String,
    pub severity: Severity,
    /// One line: what happened.
    pub title: String,
    /// Why this rule fired — the exact symptom found in the log.
    pub evidence: String,
    /// Concrete steps, in order. Never generic advice.
    pub fix: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalAnalysis {
    /// Crash headline when the log has one ("The game crashed whilst …").
    pub summary: String,
    /// Exception class and message, when present.
    pub exception: Option<String>,
    pub diagnoses: Vec<Diagnosis>,
    /// Jar names mentioned anywhere in the log.
    pub mods: Vec<String>,
    /// True when only the tail of the log was examined.
    pub truncated: bool,
    /// Bytes of the original log.
    pub bytes: usize,
}

/// How much of the log the analyser looks at. A crash report is small; a 40 MB
/// debug spam log is not, and the interesting part is always at the end.
pub const TAIL_BYTES: usize = 256 * 1024;

pub fn analyze(log: &str) -> LocalAnalysis {
    let bytes = log.len();
    let (text, truncated) = if bytes > TAIL_BYTES {
        // Cut on a char boundary: logs can contain non-ASCII mod names.
        let mut start = bytes - TAIL_BYTES;
        while start < bytes && !log.is_char_boundary(start) {
            start += 1;
        }
        (&log[start..], true)
    } else {
        (log, false)
    };

    // Every rule returns `Option<Diagnosis>`; `extend` flattens the ones that
    // fired, so adding a rule is a one-line change.
    let mut diagnoses = Vec::new();
    diagnoses.extend(out_of_memory(text));
    diagnoses.extend(unsupported_class_version(text));
    diagnoses.extend(missing_dependency(text));
    diagnoses.extend(duplicate_mods(text));
    diagnoses.extend(mixin_conflict(text));
    diagnoses.extend(class_missing(text));
    diagnoses.extend(loader_mismatch(text));
    diagnoses.extend(graphics(text));
    diagnoses.extend(access_violation(text));
    diagnoses.extend(optifine(text));
    diagnoses.extend(corrupted_jar(text));
    diagnoses.extend(account_or_session(text));

    // A crash with no recognised cause still gets an honest answer instead of
    // an empty list.
    if diagnoses.is_empty() && looks_like_crash(text) {
        diagnoses.push(Diagnosis {
            id: "unknown".to_string(),
            severity: Severity::Warning,
            title: "Log wygląda na crash, ale nie pasuje do znanych wzorców".to_string(),
            evidence: first_crash_line(text).unwrap_or_else(|| "brak wyróżnionego wpisu".into()),
            fix: vec![
                "Sprawdź, czy problem znika z pustym folderem mods (instancja bez modów).".to_string(),
                "Dodawaj mody po połowie, aż crash wróci — wtedy winowajca jest w ostatniej partii."
                    .to_string(),
                "Wyślij log do analizy AI, jeśli provider jest skonfigurowany.".to_string(),
            ],
        });
    }

    diagnoses.sort_by_key(|diagnosis| match diagnosis.severity {
        Severity::Critical => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    });

    LocalAnalysis {
        summary: first_crash_line(text).unwrap_or_else(|| "Brak wpisu o crashu w logu.".to_string()),
        exception: exception_line(text),
        diagnoses,
        mods: mentioned_mods(text),
        truncated,
        bytes,
    }
}

fn diag(
    id: &str,
    severity: Severity,
    title: &str,
    evidence: String,
    fix: &[&str],
) -> Diagnosis {
    Diagnosis {
        id: id.to_string(),
        severity,
        title: title.to_string(),
        evidence,
        fix: fix.iter().map(|line| line.to_string()).collect(),
    }
}

/// First line that contains `needle`, trimmed and shortened for display.
fn line_containing<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    text.lines()
        .find(|line| line.contains(needle))
        .map(str::trim)
}

fn first_crash_line(text: &str) -> Option<String> {
    for needle in [
        "The game crashed whilst",
        "A fatal error has been detected",
        "Encountered an unexpected exception",
        "Crash report saved to",
    ] {
        if let Some(line) = line_containing(text, needle) {
            return Some(clip(line, 240));
        }
    }
    None
}

fn exception_line(text: &str) -> Option<String> {
    // The first `SomethingException: message` after a crash header, or any line
    // that looks like a thrown exception near the top of the tail.
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with("Caused by:") || line.starts_with("java.") || line.starts_with("net.") {
            if let Some(colon) = line.find(':') {
                let head = &line[..colon];
                if head.ends_with("Exception") || head.ends_with("Error") || head.contains("Exception$")
                {
                    return Some(clip(line, 300));
                }
            } else if line.ends_with("Exception") || line.ends_with("Error") {
                return Some(clip(line, 300));
            }
        }
    }
    line_containing(text, "Exception").map(|line| clip(line, 300))
}

fn looks_like_crash(text: &str) -> bool {
    text.contains("The game crashed")
        || text.contains("crash-reports")
        || text.contains("Exception")
        || text.contains("ERROR")
}

/// Every `mods/<name>.jar` mentioned in the log, de-duplicated.
pub fn mentioned_mods(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while let Some(found) = text[index..].find("mods/") {
        let start = index + found + 5;
        let mut end = start;
        while end < bytes.len() {
            let byte = bytes[end];
            if byte == b'/' || byte == b'\\' || byte == b'"' || byte == b'\'' || byte == b' '
                || byte == b',' || byte == b')' || byte == b']' || byte == b'\n' || byte == b'\r'
            {
                break;
            }
            end += 1;
        }
        let candidate = &text[start..end];
        if candidate.ends_with(".jar") && candidate.len() < 120 {
            let name = candidate.to_string();
            if !out.contains(&name) {
                out.push(name);
            }
        }
        index = end.max(start);
        if index >= text.len() {
            break;
        }
    }
    out.truncate(40);
    out
}

fn clip(text: &str, max: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max).collect();
    out.push('…');
    out
}

/* ------------------------------------------------------------------------- */
/* Rules                                                                     */
/* ------------------------------------------------------------------------- */

fn out_of_memory(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "Java heap space")
        .or_else(|| line_containing(text, "OutOfMemoryError"))
        .or_else(|| line_containing(text, "GC overhead limit exceeded"))?;
    Some(diag(
        "out-of-memory",
        Severity::Critical,
        "Skończył się heap Javy",
        clip(marker, 240),
        &[
            "Uruchom instance → Overview → Performance → Apply, żeby dostać rekomendowany heap.",
            "Jeśli instancja ma już >8 GB, problemem bywa wyciek w modzie, nie za mały heap.",
            "Sprawdź, czy nie masz ustawionego -Xmx większego niż połowa RAM — swap wydłuża GC i kończy się tak samo.",
        ],
    ))
}

fn unsupported_class_version(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "UnsupportedClassVersionError")?;
    // `class file version 65.0` → Java 21, 61 → 17, 55 → 11, 52 → 8.
    let major = text
        .split("class file version ")
        .nth(1)
        .and_then(|rest| rest.split('.').next())
        .and_then(|value| value.trim().parse::<u32>().ok());
    let java = major.map(|major| match major {
        45..=52 => "8",
        53..=56 => "9–12",
        57..=60 => "13–16",
        61..=64 => "17–20",
        65..=68 => "21–24",
        _ => "nowszej",
    });
    let title = match java {
        Some(version) => format!("Zły runtime Javy — mod wymaga Javy {version} lub nowszej"),
        None => "Zły runtime Javy — klasa skompilowana dla nowszej wersji".to_string(),
    };
    Some(diag(
        "java-too-old",
        Severity::Critical,
        &title,
        clip(marker, 240),
        &[
            "Settings → Java → Detect, wskaż runtime pasujący do wersji gry (1.20.5+ → Java 21).",
            "Albo włącz automatyczne pobieranie Temurin i uruchom instalację jeszcze raz.",
            "Instancja → Settings → Java → wybierz konkretny runtime, nie „auto”, jeśli masz kilka.",
        ],
    ))
}

fn missing_dependency(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "requires")
        .filter(|line| line.contains("mod") || line.contains("Mod"))
        .or_else(|| line_containing(text, "Missing or unsupported mandatory dependencies"))
        .or_else(|| line_containing(text, "which is missing"))?;
    Some(diag(
        "missing-dependency",
        Severity::Critical,
        "Brakuje zależności moda",
        clip(marker, 260),
        &[
            "Przeczytaj nazwę brakującego moda z komunikatu powyżej — to zwykle `fabric-api`, `architectury` albo biblioteka konkretnego moda.",
            "Discover → wyszukaj tę nazwę, instaluj wersję zgodną z wersją gry i loaderem.",
            "Po instalacji uruchom ponownie — brakująca zależność jest zwykle jedyną przyczyną.",
        ],
    ))
}

fn duplicate_mods(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "Duplicate mods")
        .or_else(|| line_containing(text, "duplicate mod"))
        .or_else(|| line_containing(text, "already registered"))?;
    Some(diag(
        "duplicate-mod",
        Severity::Critical,
        "Ten sam mod załadowany dwa razy",
        clip(marker, 260),
        &[
            "Usuń starszą wersję jara z folderu mods — dwie wersje tego samego moda to zawsze crash lub dziwne zachowanie.",
            "Settings → Mod storage → Deduplicate nie usuwa duplikatów logicznych: to nadal dwa różne pliki.",
            "Sprawdź, czy paczka modów nie zainstalowała już tej samej biblioteki, którą dodałeś ręcznie.",
        ],
    ))
}

fn mixin_conflict(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "Mixin apply failed")
        .or_else(|| line_containing(text, "MixinTransformerError"))
        .or_else(|| line_containing(text, "InvalidMixinException"))
        .or_else(|| line_containing(text, "mixin"))?;
    Some(diag(
        "mixin-conflict",
        Severity::Critical,
        "Konflikt mixinów — dwa mody patrzą w tę samą klasę",
        clip(marker, 260),
        &[
            "W komunikacie wyżej jest nazwa mixina i moda, który go zgłosił — usuń albo zaktualizuj ten mod.",
            "Najczęstsze pary konfliktów: OptiFine + Sodium/Iris, dwa mody na to samo GUI, stara wersja Fabric API.",
            "Wyłączaj mody połówkami, aż crash zniknie — binary search po folderze mods jest najszybszy.",
        ],
    ))
}

fn class_missing(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "NoClassDefFoundError")
        .or_else(|| line_containing(text, "ClassNotFoundException"))?;
    Some(diag(
        "class-missing",
        Severity::Critical,
        "Brakuje klasy — najczęściej brakująca biblioteka lub niezgodna wersja moda",
        clip(marker, 260),
        &[
            "Nazwa klasy wskazuje zwykle bibliotekę: `com/…` → brakujący mod, `net/minecraft/…` → mod na inną wersję gry.",
            "Sprawdź, czy mod jest na tę samą wersję Minecrafta i ten sam loader (Fabric ≠ Forge ≠ NeoForge).",
            "Zaktualizuj Fabric API / Architectury / Cloth Config — najczęstsze źródło brakujących klas.",
        ],
    ))
}

fn loader_mismatch(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "is not a valid mod file")
        .or_else(|| line_containing(text, "Incompatible mod set"))
        .or_else(|| line_containing(text, "requires forge"))
        .or_else(|| line_containing(text, "requires fabric"))?;
    Some(diag(
        "loader-mismatch",
        Severity::Critical,
        "Mod jest na inny loader lub inną wersję gry",
        clip(marker, 260),
        &[
            "Sprawdź plik `fabric.mod.json` / `mods.toml` w jarze — tam jest wymagany loader i wersja gry.",
            "Nie mieszaj jarów Forge i Fabric w jednym folderze mods.",
            "Na Fabric instaluj tylko buildy oznaczone jako Fabric, na NeoForge tylko NeoForge.",
        ],
    ))
}

fn graphics(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "GLFW error")
        .or_else(|| line_containing(text, "Failed to create window"))
        .or_else(|| line_containing(text, "OpenGL"))
        .or_else(|| line_containing(text, "Pixel format not accelerated"))
        .or_else(|| line_containing(text, "Couldn't set pixel format"))?;
    Some(diag(
        "graphics",
        Severity::Warning,
        "Problem z OpenGL / sterownikiem karty graficznej",
        clip(marker, 240),
        &[
            "Zaktualizuj sterownik GPU — to odpowiada za większość błędów GLFW na Windows.",
            "Na laptopie z dwiema kartami wymuś kartę dedykowaną dla javaw.exe w panelu sterownika.",
            "Jeśli używasz shaderów, wyłącz je w options.txt (`enableShaders:false`) i sprawdź ponownie.",
        ],
    ))
}

fn access_violation(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "EXCEPTION_ACCESS_VIOLATION")
        .or_else(|| line_containing(text, "0xc0000005"))
        .or_else(|| line_containing(text, "-1073741819"))?;
    Some(diag(
        "native-crash",
        Severity::Critical,
        "Crash natywny (access violation), nie wyjątek Javy",
        clip(marker, 240),
        &[
            "Winowajcą jest zwykle mod z natywnym kodem (renderery, shadery, sterowniki) — nie błąd Javy.",
            "Wyłącz mody graficzne: Sodium/Iris/Embeddium/Oculus i sprawdź, czy crash wraca.",
            "Zaktualizuj Javę i sterownik GPU; crash natywny na starej Javie 8 zdarza się częściej.",
        ],
    ))
}

fn optifine(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "OptiFine").or_else(|| line_containing(text, "optifine"))?;
    Some(diag(
        "optifine",
        Severity::Warning,
        "OptiFine w zestawie — częste źródło konfliktów",
        clip(marker, 200),
        &[
            "Na Fabric użyj Sodium + Iris zamiast OptiFine; na NeoForge — Embeddium/Oculus.",
            "Jeśli OptiFine musi zostać, usuń inne mody dotykające renderowania.",
        ],
    ))
}

fn corrupted_jar(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "ZipException")
        .or_else(|| line_containing(text, "invalid LOC header"))
        .or_else(|| line_containing(text, "Error reading"))?;
    Some(diag(
        "corrupted-jar",
        Severity::Critical,
        "Uszkodzony plik jara",
        clip(marker, 240),
        &[
            "Usuń wskazany plik i zainstaluj moda ponownie — pobieranie zostało przerwane.",
            "Settings → Mod storage → Check for unused blobs, potem Deduplicate, żeby odświeżyć linki.",
            "Jeśli plik pochodzi z paczki, przeinstaluj paczkę zamiast poprawiać pojedynczy jar.",
        ],
    ))
}

fn account_or_session(text: &str) -> Option<Diagnosis> {
    let marker = line_containing(text, "Invalid credentials")
        .or_else(|| line_containing(text, "AuthenticationException"))
        .or_else(|| line_containing(text, "Failed to verify username"))
        .or_else(|| line_containing(text, "Invalid session"))?;
    Some(diag(
        "auth",
        Severity::Warning,
        "Problem z logowaniem / sesją konta",
        clip(marker, 220),
        &[
            "Accounts → wyloguj i zaloguj ponownie; token sesji wygasa i nie jest odświeżany w tle.",
            "Na serwerze online-mode konto offline nie wejdzie — to nie błąd launchera.",
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_memory_is_reported_with_heap_advice() {
        let log = "java.lang.OutOfMemoryError: Java heap space\n\tat net.minecraft.server.MinecraftServer.tick";
        let analysis = analyze(log);
        assert_eq!(analysis.diagnoses[0].id, "out-of-memory");
        assert!(analysis.diagnoses[0]
            .fix
            .iter()
            .any(|step| step.contains("Performance")));
    }

    #[test]
    fn class_file_version_maps_to_java_release() {
        let log = "java.lang.UnsupportedClassVersionError: net/fabricmc/loader/impl/FabricLoaderImpl has been compiled by a more recent version of the Java Runtime (class file version 65.0)";
        let analysis = analyze(log);
        let diagnosis = &analysis.diagnoses[0];
        assert_eq!(diagnosis.id, "java-too-old");
        assert!(diagnosis.title.contains("21"));
    }

    #[test]
    fn missing_dependency_is_critical() {
        let log = "net.fabricmc.loader.impl.FormattedException: Mod 'sodium' (0.5.11) requires version 0.92.0 or later of fabric-api, which is missing!";
        let analysis = analyze(log);
        assert!(analysis
            .diagnoses
            .iter()
            .any(|diagnosis| diagnosis.id == "missing-dependency"));
    }

    #[test]
    fn mods_are_extracted_from_paths() {
        let log = "Loading mods/sodium-0.5.11.jar\nLoading mods/fabric-api-0.92.0.jar\nmods/sodium-0.5.11.jar";
        let mods = mentioned_mods(log);
        assert_eq!(mods, vec!["sodium-0.5.11.jar", "fabric-api-0.92.0.jar"]);
    }

    #[test]
    fn long_logs_are_truncated_to_the_tail() {
        let padding = "x".repeat(TAIL_BYTES + 5000);
        let log = format!("{padding}\njava.lang.OutOfMemoryError: Java heap space");
        let analysis = analyze(&log);
        assert!(analysis.truncated);
        // The tail still contains the real cause.
        assert_eq!(analysis.diagnoses[0].id, "out-of-memory");
    }

    #[test]
    fn unknown_crash_still_gets_an_answer() {
        let log = "---- Minecraft Crash Report ----\nThe game crashed whilst ticking entity\njava.lang.RuntimeException: boom";
        let analysis = analyze(log);
        assert!(!analysis.diagnoses.is_empty());
        assert!(analysis
            .diagnoses
            .iter()
            .any(|diagnosis| diagnosis.id == "unknown"));
    }

    #[test]
    fn clean_log_produces_no_diagnoses() {
        let analysis = analyze("[12:00:00] [main/INFO]: Loading Minecraft 1.21.1\n[12:00:01] [main/INFO]: Done");
        assert!(analysis.diagnoses.is_empty());
    }
}
