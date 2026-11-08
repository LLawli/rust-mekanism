//! Um arquivo de configuração de cliente como o `ModConfigSpec` do NeoForge o escreve, lê e corrige. Sem tipos do jogo: o mod dá a
//! especificação (seções, chaves, padrões, limites e comentários), o jogo dá o texto do arquivo, e daqui sai o valor de cada chave e,
//! quando algum estava errado, o texto corrigido.
//!
//! O que o NeoForge faz e isto repete (`ModConfigSpec.java`):
//! - um número fora do limite vai para o limite (`Range.correct`, linha 1054), e um valor de outro tipo ou ausente vai para o padrão
//!   (`ValueSpec.correct`, linha 1132); um inteiro vale como número (`Range.isNumber`, linha 1030);
//! - um booleano aceita o texto `true` ou `false` em qualquer caixa (o validador de `define(String, boolean)`, linha 755);
//! - ao carregar, um arquivo com algum valor corrigido é escrito de novo (`correct`, linha 213, e o `config.save()` do `acceptConfig`, linha 119). O
//!   original apaga do arquivo as chaves que a especificação dele não conhece, e a especificação dele é a do mod inteiro. Aqui o arquivo também guarda
//!   as posições das janelas, que outro código escreve (`replace_sections`): `rewrite` troca só as seções da especificação e deixa as outras;
//! - o texto que o NightConfig escreve: o comentário da seção sem espaço depois do `#`, uma tabulação por nível, `# Default:` e
//!   `# Range:` só nos números, uma linha em branco entre as seções.

/// O que uma chave aceita.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Setting {
    /// `define(chave, padrão)`.
    Bool { default: bool },
    /// `defineInRange(chave, padrão, mínimo, máximo)` de `double`.
    Float { default: f64, min: f64, max: f64 },
}

/// Uma chave de uma seção.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub key: &'static str,
    pub comment: &'static str,
    pub setting: Setting,
}

/// Uma seção (`builder.push`), com o comentário que o `applyToBuilder` dá.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub name: &'static str,
    pub comment: &'static str,
    pub entries: &'static [Entry],
}

/// O valor de uma chave, depois da correção.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Float(f64),
}

/// A especificação de um arquivo.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub sections: &'static [Section],
}

/// O arquivo lido: o valor de cada chave (na ordem da especificação) e se o texto precisou de correção.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    values: Vec<Vec<Value>>,
    /// O NeoForge escreve o arquivo de novo quando isto é verdadeiro.
    pub corrected: bool,
}

impl Spec {
    /// O valor de cada chave quando o arquivo não diz nada.
    pub fn defaults(&self) -> Loaded {
        Loaded {
            values: self
                .sections
                .iter()
                .map(|section| {
                    section
                        .entries
                        .iter()
                        .map(|entry| default_of(entry.setting))
                        .collect()
                })
                .collect(),
            corrected: false,
        }
    }

    /// O texto de um arquivo que ainda não existe.
    pub fn default_text(&self) -> String {
        self.render(&self.defaults())
    }

    /// O texto do arquivo com estes valores, como o NightConfig o escreve.
    pub fn render(&self, loaded: &Loaded) -> String {
        let mut text = String::new();
        for index in 0..self.sections.len() {
            if index > 0 {
                text.push('\n');
            }
            text.push_str(&self.render_section(index, loaded));
        }
        text
    }

    /// O texto de uma seção: o comentário, o cabeçalho e as chaves.
    fn render_section(&self, index: usize, loaded: &Loaded) -> String {
        let section = &self.sections[index];
        let mut text = format!("#{}\n[{}]\n", section.comment, section.name);
        for (entry, value) in section.entries.iter().zip(&loaded.values[index]) {
            text.push_str(&format!("\t#{}\n", entry.comment));
            if let Setting::Float { default, min, max } = entry.setting {
                text.push_str(&format!(
                    "\t# Default: {}\n\t# Range: {} ~ {}\n",
                    float_text(default),
                    float_text(min),
                    float_text(max)
                ));
            }
            text.push_str(&format!("\t{} = {}\n", entry.key, value_text(*value)));
        }
        text
    }

    /// O arquivo de `text` com as seções da especificação no lugar, corrigidas, e tudo o mais como estava: um arquivo que outro código do mod também
    /// escreve (as posições das janelas) não perde as seções dele. O que a especificação ainda não tem entra no fim.
    pub fn rewrite(&self, text: &str, loaded: &Loaded) -> String {
        let mut written = vec![false; self.sections.len()];
        let mut out: Vec<String> = Vec::new();
        for chunk in chunks(text) {
            let known = chunk.header.as_deref().and_then(|name| {
                self.sections
                    .iter()
                    .position(|section| section.name == name)
            });
            match known {
                Some(index) if !written[index] => {
                    written[index] = true;
                    out.push(self.render_section(index, loaded));
                }
                Some(_) => {}
                None => out.push(chunk.lines.join("\n") + "\n"),
            }
        }
        for (index, done) in written.iter().enumerate() {
            if !done {
                out.push(self.render_section(index, loaded));
            }
        }
        join_blocks(out)
    }

    /// Lê o texto de um arquivo e corrige o que está errado.
    pub fn load(&self, text: &str) -> Loaded {
        let found = parse(text);
        let mut corrected = false;
        let values = self
            .sections
            .iter()
            .map(|section| {
                section
                    .entries
                    .iter()
                    .map(|entry| {
                        let raw = found
                            .iter()
                            .find(|(name, key, _)| *name == section.name && *key == entry.key)
                            .map(|(_, _, raw)| raw);
                        let (value, fixed) = correct(entry.setting, raw);
                        corrected |= fixed;
                        value
                    })
                    .collect()
            })
            .collect();
        Loaded { values, corrected }
    }
}

impl Loaded {
    fn value(&self, spec: &Spec, section: &str, key: &str) -> Option<Value> {
        let section_index = spec
            .sections
            .iter()
            .position(|candidate| candidate.name == section)?;
        let entry_index = spec.sections[section_index]
            .entries
            .iter()
            .position(|candidate| candidate.key == key)?;
        self.values[section_index].get(entry_index).copied()
    }

    /// O valor de uma chave booleana.
    pub fn bool(&self, spec: &Spec, section: &str, key: &str) -> Option<bool> {
        match self.value(spec, section, key)? {
            Value::Bool(value) => Some(value),
            Value::Float(_) => None,
        }
    }

    /// O valor de uma chave numérica.
    pub fn float(&self, spec: &Spec, section: &str, key: &str) -> Option<f64> {
        match self.value(spec, section, key)? {
            Value::Float(value) => Some(value),
            Value::Bool(_) => None,
        }
    }
}

/// Um pedaço do arquivo: de um cabeçalho `[seção]` (com os comentários sem recuo logo acima) até o próximo.
struct Chunk {
    header: Option<String>,
    lines: Vec<String>,
}

fn header_of(line: &str) -> Option<String> {
    let inner = line.trim().strip_prefix('[')?;
    Some(inner.split(']').next()?.trim().to_owned())
}

fn chunks(text: &str) -> Vec<Chunk> {
    let mut out = vec![Chunk {
        header: None,
        lines: Vec::new(),
    }];
    for line in text.lines() {
        if let Some(header) = header_of(line) {
            let current = out.last_mut().expect("sempre há um pedaço");
            // Os comentários da seção ficam logo acima do cabeçalho, sem recuo.
            let mut comments = Vec::new();
            while current
                .lines
                .last()
                .is_some_and(|last| last.starts_with('#'))
            {
                comments.push(current.lines.pop().expect("conferido"));
            }
            comments.reverse();
            comments.push(line.to_owned());
            out.push(Chunk {
                header: Some(header),
                lines: comments,
            });
        } else {
            out.last_mut()
                .expect("sempre há um pedaço")
                .lines
                .push(line.to_owned());
        }
    }
    out.retain(|chunk| chunk.lines.iter().any(|line| !line.trim().is_empty()));
    out
}

/// Junta os blocos com uma linha em branco entre eles e termina em `\n`.
fn join_blocks(blocks: Vec<String>) -> String {
    blocks
        .iter()
        .map(|block| block.trim_end_matches('\n'))
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
        + "\n"
}

/// `text` sem as seções `prefix` e `prefix.*` e com `new` no fim: quem escreve só as suas seções de um arquivo partilhado (as posições das janelas,
/// `window.<nome>`) não apaga as dos outros.
pub fn replace_sections(text: &str, prefix: &str, new: &str) -> String {
    let dotted = format!("{prefix}.");
    let mut blocks: Vec<String> = chunks(text)
        .into_iter()
        .filter(|chunk| {
            !chunk
                .header
                .as_deref()
                .is_some_and(|name| name == prefix || name.starts_with(&dotted))
        })
        .map(|chunk| chunk.lines.join("\n"))
        .collect();
    blocks.push(new.to_owned());
    join_blocks(blocks)
}

fn default_of(setting: Setting) -> Value {
    match setting {
        Setting::Bool { default } => Value::Bool(default),
        Setting::Float { default, .. } => Value::Float(default),
    }
}

/// `Double.toString` para os valores que um arquivo de configuração traz: `1.0`, `0.5`, `10.0`.
fn float_text(value: f64) -> String {
    format!("{value:?}")
}

fn value_text(value: Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Float(value) => float_text(value),
    }
}

/// O que o arquivo diz de uma chave, antes de a especificação olhar.
#[derive(Clone, Debug, PartialEq)]
enum Raw {
    Bool(bool),
    Number(f64),
    Text(String),
}

/// O subconjunto de TOML que uma configuração de cliente usa: `[seção]`, `chave = valor`, comentários com `#` e linhas em branco.
/// Uma chave ou seção com ponto, aspas ou outra forma é ignorada, e o texto de uma linha que não se entende também.
fn parse(text: &str) -> Vec<(String, String, Raw)> {
    let mut found = Vec::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            section = rest.split(']').next().unwrap_or("").trim().to_owned();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        if let Some(raw) = parse_value(value.trim()) {
            found.push((section.clone(), key.to_owned(), raw));
        }
    }
    found
}

fn parse_value(value: &str) -> Option<Raw> {
    if let Some(quoted) = value.strip_prefix('"') {
        return Some(Raw::Text(quoted.split('"').next().unwrap_or("").to_owned()));
    }
    if let Some(quoted) = value.strip_prefix('\'') {
        return Some(Raw::Text(
            quoted.split('\'').next().unwrap_or("").to_owned(),
        ));
    }
    // Um comentário depois do valor.
    let value = value.split('#').next().unwrap_or("").trim();
    match value {
        "true" => return Some(Raw::Bool(true)),
        "false" => return Some(Raw::Bool(false)),
        "inf" | "+inf" => return Some(Raw::Number(f64::INFINITY)),
        "-inf" => return Some(Raw::Number(f64::NEG_INFINITY)),
        _ => {}
    }
    let digits: String = value.chars().filter(|c| *c != '_').collect();
    digits
        .parse::<f64>()
        .ok()
        .filter(|number| !number.is_nan())
        .map(Raw::Number)
}

/// `ValueSpec.test` e `correct` de uma chave. Devolve o valor e se foi preciso corrigi-lo.
fn correct(setting: Setting, raw: Option<&Raw>) -> (Value, bool) {
    match setting {
        Setting::Bool { default } => match raw {
            Some(Raw::Bool(value)) => (Value::Bool(*value), false),
            // O validador aceita o texto `true` ou `false`, de qualquer caixa. O Java guarda a `String` e falha ao ler o booleano
            // (`ClassCastException`); aqui vale o que o texto diz.
            Some(Raw::Text(text)) if text.eq_ignore_ascii_case("true") => {
                (Value::Bool(true), false)
            }
            Some(Raw::Text(text)) if text.eq_ignore_ascii_case("false") => {
                (Value::Bool(false), false)
            }
            _ => (Value::Bool(default), true),
        },
        Setting::Float { default, min, max } => match raw {
            Some(Raw::Number(number)) if (min..=max).contains(number) => {
                (Value::Float(*number), false)
            }
            Some(Raw::Number(number)) if *number < min => (Value::Float(min), true),
            Some(Raw::Number(_)) => (Value::Float(max), true),
            _ => (Value::Float(default), true),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: Spec = Spec {
        sections: &[Section {
            name: "sounds",
            comment: "Settings for configuring Mekanism's Sounds",
            entries: &[
                Entry {
                    key: "baseVolume",
                    comment: "Adjust Mekanism sounds' base volume. < 1 is softer, higher is louder.",
                    setting: Setting::Float {
                        default: 1.0,
                        min: 0.0,
                        max: 10.0,
                    },
                },
                Entry {
                    key: "enableMachine",
                    comment: "Play sounds for active machines.",
                    setting: Setting::Bool { default: true },
                },
            ],
        }],
    };

    fn volume(loaded: &Loaded) -> f64 {
        loaded.float(&SPEC, "sounds", "baseVolume").unwrap()
    }

    fn machine(loaded: &Loaded) -> bool {
        loaded.bool(&SPEC, "sounds", "enableMachine").unwrap()
    }

    #[test]
    fn the_default_file_is_what_the_original_writes() {
        assert_eq!(
            SPEC.default_text(),
            "#Settings for configuring Mekanism's Sounds\n[sounds]\n\t#Adjust Mekanism sounds' base volume. < 1 is softer, higher is louder.\n\t# Default: 1.0\n\t# Range: 0.0 ~ 10.0\n\tbaseVolume = 1.0\n\t#Play sounds for active machines.\n\tenableMachine = true\n"
        );
    }

    #[test]
    fn the_default_file_reads_back_as_correct_with_the_defaults() {
        let loaded = SPEC.load(&SPEC.default_text());
        assert!(!loaded.corrected);
        assert_eq!(loaded, SPEC.defaults());
        assert_eq!(volume(&loaded), 1.0);
        assert!(machine(&loaded));
    }

    #[test]
    fn a_number_in_range_is_kept_and_an_integer_counts_as_a_number() {
        let loaded = SPEC.load("[sounds]\n\tbaseVolume = 0.25\n\tenableMachine = false\n");
        assert_eq!(volume(&loaded), 0.25);
        assert!(!machine(&loaded));
        assert!(!loaded.corrected);
        let loaded = SPEC.load("[sounds]\nbaseVolume = 3\n");
        assert_eq!(volume(&loaded), 3.0);
        // O limite de cima e o de baixo valem.
        assert_eq!(volume(&SPEC.load("[sounds]\nbaseVolume = 10.0\n")), 10.0);
        assert_eq!(volume(&SPEC.load("[sounds]\nbaseVolume = 0\n")), 0.0);
    }

    #[test]
    fn a_number_out_of_range_goes_to_the_limit_and_the_file_is_written_again() {
        let loaded = SPEC.load("[sounds]\nbaseVolume = 12.5\nenableMachine = true\n");
        assert_eq!(volume(&loaded), 10.0);
        assert!(loaded.corrected);
        let loaded = SPEC.load("[sounds]\nbaseVolume = -3\nenableMachine = true\n");
        assert_eq!(volume(&loaded), 0.0);
        assert!(loaded.corrected);
        let loaded = SPEC.load("[sounds]\nbaseVolume = inf\nenableMachine = true\n");
        assert_eq!(volume(&loaded), 10.0);
        // O arquivo corrigido lê limpo.
        let again = SPEC.load(&SPEC.render(&loaded));
        assert!(!again.corrected);
        assert_eq!(volume(&again), 10.0);
    }

    #[test]
    fn a_missing_or_wrong_type_value_goes_to_the_default() {
        let loaded = SPEC.load("");
        assert_eq!((volume(&loaded), machine(&loaded)), (1.0, true));
        assert!(loaded.corrected);
        let loaded = SPEC.load("[sounds]\nbaseVolume = true\nenableMachine = 5\n");
        assert_eq!((volume(&loaded), machine(&loaded)), (1.0, true));
        assert!(loaded.corrected);
        let loaded = SPEC.load("[sounds]\nbaseVolume = \"loud\"\nenableMachine = false\n");
        assert_eq!((volume(&loaded), machine(&loaded)), (1.0, false));
        assert!(loaded.corrected);
        // Fora da seção certa a chave não conta.
        let loaded = SPEC.load("[other]\nbaseVolume = 4\nenableMachine = false\n");
        assert_eq!((volume(&loaded), machine(&loaded)), (1.0, true));
    }

    #[test]
    fn a_boolean_written_as_text_is_accepted_in_any_case() {
        let loaded = SPEC.load("[sounds]\nbaseVolume = 1.0\nenableMachine = \"FALSE\"\n");
        assert!(!machine(&loaded));
        assert!(!loaded.corrected);
    }

    #[test]
    fn a_comment_after_the_value_and_unknown_keys_do_not_matter() {
        let loaded = SPEC.load("# a\n[sounds]\n\tbaseVolume = 2.5 # alto\n\tother = 1\n\tenableMachine = false # mudo\n");
        assert_eq!((volume(&loaded), machine(&loaded)), (2.5, false));
        assert!(!loaded.corrected);
    }

    #[test]
    fn the_rewritten_file_drops_what_the_spec_does_not_know() {
        let loaded = SPEC.load("[sounds]\nbaseVolume = 99\nenableMachine = true\nenablePlayer = false\n[rendering]\nberRange = 4\n");
        let text = SPEC.render(&loaded);
        assert!(!text.contains("enablePlayer"));
        assert!(!text.contains("rendering"));
        assert!(text.contains("baseVolume = 10.0"));
    }

    #[test]
    fn rewriting_keeps_the_sections_the_spec_does_not_know() {
        let text = "[window.upgrade]\nx = -11\ny = 15\npinned = false\n\n#Settings for configuring Mekanism's Sounds\n[sounds]\n\tbaseVolume = 99\n\tenableMachine = true\n";
        let loaded = SPEC.load(text);
        assert!(loaded.corrected);
        let written = SPEC.rewrite(text, &loaded);
        assert!(written.starts_with("[window.upgrade]\nx = -11\ny = 15\npinned = false\n\n#Settings for configuring Mekanism's Sounds\n[sounds]\n"));
        assert!(written.contains("baseVolume = 10.0"));
        // O arquivo reescrito lê limpo e uma segunda reescrita não o muda.
        let again = SPEC.load(&written);
        assert!(!again.corrected);
        assert_eq!(SPEC.rewrite(&written, &again), written);
    }

    #[test]
    fn rewriting_adds_a_missing_section_at_the_end() {
        let text = "[window.upgrade]\nx = 1\ny = 2\npinned = true\n";
        let loaded = SPEC.load(text);
        assert!(loaded.corrected, "falta a seção sounds");
        let written = SPEC.rewrite(text, &loaded);
        assert_eq!(written, format!("{text}\n{}", SPEC.default_text()));
        assert!(!SPEC.load(&written).corrected);
    }

    #[test]
    fn replacing_a_prefix_keeps_the_other_sections() {
        let text = SPEC.default_text()
            + "\n[window.upgrade]\nx = 1\ny = 2\npinned = true\n\n[window.other]\nx = 3\ny = 4\npinned = false\n";
        let merged = replace_sections(
            &text,
            "window",
            "[window.upgrade]\nx = 9\ny = 8\npinned = false\n",
        );
        assert_eq!(
            merged,
            SPEC.default_text() + "\n[window.upgrade]\nx = 9\ny = 8\npinned = false\n"
        );
        assert_eq!(
            replace_sections("", "window", "[window.a]\nx = 1\n"),
            "[window.a]\nx = 1\n"
        );
        // Um arquivo só com posições vira só as novas.
        assert_eq!(
            replace_sections("[window.a]\nx = 1\n", "window", "[window.b]\nx = 2\n"),
            "[window.b]\nx = 2\n"
        );
    }

    #[test]
    fn sections_are_separated_by_a_blank_line() {
        const TWO: Spec = Spec {
            sections: &[
                Section {
                    name: "a",
                    comment: "First",
                    entries: &[Entry {
                        key: "x",
                        comment: "X.",
                        setting: Setting::Bool { default: false },
                    }],
                },
                Section {
                    name: "b",
                    comment: "Second",
                    entries: &[Entry {
                        key: "y",
                        comment: "Y.",
                        setting: Setting::Bool { default: true },
                    }],
                },
            ],
        };
        assert_eq!(
            TWO.default_text(),
            "#First\n[a]\n\t#X.\n\tx = false\n\n#Second\n[b]\n\t#Y.\n\ty = true\n"
        );
    }
}
