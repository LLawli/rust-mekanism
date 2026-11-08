//! Um arquivo de configuração como o `ModConfigSpec` do NeoForge o escreve, lê e corrige, de cliente ou de servidor. Sem tipos do jogo: o mod dá a
//! especificação (seções, chaves, padrões, limites e comentários), o jogo dá o texto do arquivo, e daqui sai o valor de cada chave e, quando algo estava
//! errado, o texto corrigido. O nome do módulo é o da primeira versão (só o arquivo de cliente); a regra é a mesma para os arquivos de servidor.
//!
//! O que o NeoForge faz e isto repete (`ModConfigSpec.java`; `rastros/config_correcao` tem os arquivos que o original corrigiu de verdade):
//! - um número fora do limite vai para o limite (`Range.correct`, linha 1054), e um valor de outro tipo ou ausente vai para o padrão
//!   (`ValueSpec.correct`, linha 1132); um inteiro vale como número (`Range.isNumber`, linha 1030). Um inteiro de `defineInRange(int)` que o arquivo
//!   escreve com ponto (`3.0`) passa no teste e o Java falha ao lê-lo (`ClassCastException`): aqui vale o padrão;
//! - uma chave de `define(chave, padrão, validador)` sem faixa (`correct` devolve o padrão, linha 1133) vai para o padrão quando o validador recusa;
//! - um booleano aceita o texto `true` ou `false` em qualquer caixa (o validador de `define(String, boolean)`, linha 755);
//! - o arquivo é escrito de novo quando algo precisou de correção: valor, chave ou seção que falta (`correct`, linha 213), ou comentário que não é o da
//!   especificação (`stringsMatchNormalizingNewLines`, linha 296). O original apaga do arquivo o que a especificação dele não conhece, e a especificação dele é
//!   a do mod inteiro. A daqui é a das chaves que o port lê, então `rewrite` guarda as chaves e as seções que ela não tem (as de um arquivo que veio do mod de
//!   verdade) e não as conta como erro;
//! - o texto que o NightConfig escreve: o comentário da seção sem espaço depois do `#`, uma tabulação por nível, `# Default:` e `# Range:` só nos
//!   números de `defineInRange`, as chaves de fora de seção no começo sem recuo, uma linha em branco entre as seções e antes de cada sub-seção.
//!
//! Uma seção com ponto no nome (`rendering.particle`) é uma sub-seção da que vem antes dela (`rendering`), um nível a mais de recuo. O nome vazio é a raiz: as
//! chaves de fora de qualquer seção.

/// O que uma chave aceita.
#[derive(Clone, Copy, Debug)]
pub enum Setting {
    /// `define(chave, padrão)`.
    Bool { default: bool },
    /// `defineInRange(chave, padrão, mínimo, máximo)` de `double`.
    Float { default: f64, min: f64, max: f64 },
    /// `defineInRange(chave, padrão, mínimo, máximo)` de `int`.
    Int { default: i32, min: i32, max: i32 },
    /// `defineInRange(chave, padrão, mínimo, máximo)` de `long`.
    Long { default: i64, min: i64, max: i64 },
    /// `define(chave, padrão, validador)` de `int`: sem faixa, e o valor que o validador recusa vira o padrão. O validador pode olhar outra chave do arquivo
    /// (`heatGenerationFluidRate <= heatTankCapacity.getOrDefault()`).
    IntWhere {
        default: i32,
        valid: fn(i32, Lookup<'_>) -> bool,
    },
}

/// Onde um validador olha o valor de outra chave (seção e chave): o `getOrDefault()` do original.
pub type Lookup<'a> = &'a dyn Fn(&str, &str) -> Option<Value>;

impl Setting {
    /// Se o `defineInRange` escreve `# Default:` e `# Range:` nos comentários.
    fn has_range(&self) -> bool {
        !matches!(self, Self::Bool { .. } | Self::IntWhere { .. })
    }
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
    Int(i32),
    Long(i64),
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
        join_blocks(
            (0..self.sections.len())
                .map(|index| self.render_section(index, loaded))
                .collect(),
        )
    }

    /// O texto de uma seção: o comentário, o cabeçalho e as chaves, recuados pelo nível dela.
    fn render_section(&self, index: usize, loaded: &Loaded) -> String {
        self.render_section_over(index, loaded, None)
    }

    /// O mesmo sobre o que o arquivo já tem na seção: as chaves ficam na ordem do arquivo (o NightConfig mantém a ordem de quem já estava, e acrescenta no fim
    /// as que faltavam), as que a especificação não conhece ficam como estão, e a duplicada some.
    fn render_section_over(&self, index: usize, loaded: &Loaded, chunk: Option<&Chunk>) -> String {
        let section = &self.sections[index];
        let depth = depth_of(section.name);
        let mut text = String::new();
        if depth > 0 {
            text.push_str(&comment_text(section.comment, depth - 1));
            text.push_str(&format!("{}[{}]\n", "\t".repeat(depth - 1), section.name));
        }
        let mut done = vec![false; section.entries.len()];
        for found in chunk.map_or(&[][..], |chunk| &chunk.entries[..]) {
            match section
                .entries
                .iter()
                .position(|entry| entry.key == found.key)
            {
                Some(position) if !done[position] => {
                    done[position] = true;
                    text.push_str(&entry_text(
                        &section.entries[position],
                        loaded.values[index][position],
                        depth,
                    ));
                }
                Some(_) => {}
                None => {
                    text.push_str(&found.lines.join("\n"));
                    text.push('\n');
                }
            }
        }
        for (position, entry) in section.entries.iter().enumerate() {
            if !done[position] {
                text.push_str(&entry_text(entry, loaded.values[index][position], depth));
            }
        }
        text
    }

    /// O arquivo de `text` com as seções da especificação no lugar, corrigidas, e tudo o mais como estava: as chaves e as seções que a especificação não
    /// conhece (o mod de verdade tem muitas mais, e as posições das janelas são de outro código) não se perdem. O que a especificação tem e o arquivo não
    /// entra antes da primeira seção que a segue na especificação ou do bloco das janelas, e no fim se nenhuma existe.
    pub fn rewrite(&self, text: &str, loaded: &Loaded) -> String {
        let chunks = parse_chunks(text);
        let mut written = vec![false; self.sections.len()];
        let mut out: Vec<String> = Vec::new();
        for chunk in &chunks {
            let known = self
                .sections
                .iter()
                .position(|section| section.name == chunk.name);
            match known {
                Some(index) if !written[index] => {
                    written[index] = true;
                    out.push(self.render_section_over(index, loaded, Some(chunk)));
                }
                Some(_) => {}
                None => out.push(chunk.lines.join("\n") + "\n"),
            }
        }
        for index in (0..self.sections.len()).rev() {
            if written[index] {
                continue;
            }
            // Antes do primeiro bloco que a especificação põe depois desta seção, ou das janelas; no fim se não houver. A raiz vai ao começo.
            let follows = |name: &str| {
                name.starts_with("window")
                    || self.sections[index + 1..]
                        .iter()
                        .any(|section| section.name == name)
            };
            let at = if self.sections[index].name.is_empty() {
                0
            } else {
                out.iter()
                    .position(|block| {
                        let name = block_name(block);
                        !name.is_empty() && follows(&name)
                    })
                    .unwrap_or(out.len())
            };
            out.insert(at, self.render_section(index, loaded));
        }
        join_blocks(out)
    }

    /// Lê o texto de um arquivo e corrige o que está errado. Um validador que olha outra chave vê os padrões: é o `getOrDefault()` de um arquivo que ainda não
    /// foi lido.
    pub fn load(&self, text: &str) -> Loaded {
        self.load_after(text, None)
    }

    /// Lê de novo um arquivo que já foi lido: um validador que olha outra chave vê o valor que `previous` tem dela (o `getOrDefault()` do original devolve o
    /// valor em cache quando a configuração está carregada, e esse é o de antes da leitura).
    pub fn load_after(&self, text: &str, previous: Option<&Loaded>) -> Loaded {
        let chunks = parse_chunks(text);
        let lookup = |section: &str, key: &str| -> Option<Value> {
            match previous {
                Some(loaded) => loaded.value(self, section, key),
                None => self.default_value(section, key),
            }
        };
        let mut corrected = false;
        let mut values = Vec::with_capacity(self.sections.len());
        for section in self.sections {
            let chunk = chunks.iter().find(|chunk| chunk.name == section.name);
            if !section.name.is_empty()
                && !chunk
                    .is_some_and(|chunk| comments_match(&chunk.header_comments, section.comment))
            {
                corrected = true;
            }
            let mut row = Vec::with_capacity(section.entries.len());
            for entry in section.entries {
                let found = chunk
                    .and_then(|chunk| chunk.entries.iter().find(|found| found.key == entry.key));
                let (value, fixed) = correct(
                    entry.setting,
                    found.and_then(|found| found.value.as_ref()),
                    &lookup,
                );
                corrected |= fixed;
                if let Some(found) = found
                    && !comments_match(&found.comments, &entry_comment(entry).join("\n"))
                {
                    corrected = true;
                }
                row.push(value);
            }
            values.push(row);
        }
        Loaded { values, corrected }
    }

    fn default_value(&self, section: &str, key: &str) -> Option<Value> {
        let section = self
            .sections
            .iter()
            .find(|candidate| candidate.name == section)?;
        let entry = section
            .entries
            .iter()
            .find(|candidate| candidate.key == key)?;
        Some(default_of(entry.setting))
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
            _ => None,
        }
    }

    /// O valor de uma chave numérica de `double`.
    pub fn float(&self, spec: &Spec, section: &str, key: &str) -> Option<f64> {
        match self.value(spec, section, key)? {
            Value::Float(value) => Some(value),
            _ => None,
        }
    }

    /// O valor de uma chave de `int`.
    pub fn int(&self, spec: &Spec, section: &str, key: &str) -> Option<i32> {
        match self.value(spec, section, key)? {
            Value::Int(value) => Some(value),
            _ => None,
        }
    }

    /// O valor de uma chave de `long`.
    pub fn long(&self, spec: &Spec, section: &str, key: &str) -> Option<i64> {
        match self.value(spec, section, key)? {
            Value::Long(value) => Some(value),
            _ => None,
        }
    }
}

/// Quantas cópias de um arquivo corrigido o NeoForge guarda (`ConfigFileTypeHandler.backUpConfig`).
pub const MAX_BACKUPS: u32 = 5;

/// O nome da cópia `n` de `file_name`: `general.toml` vira `general-1.toml.bak`.
pub fn backup_name(file_name: &str, n: u32) -> String {
    match file_name.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem}-{n}.{extension}.bak"),
        None => format!("{file_name}-{n}.bak"),
    }
}

/// A cópia que o NeoForge faz antes de escrever de novo um arquivo que precisou de correção: o de antes vai para `<nome>-1.toml.bak`, as cópias que já havia
/// descem um número (`-1` para `-2`, e assim por diante), e a de número `max_backups` sai. Medido no original: um arquivo errado de propósito, corrigido
/// sete vezes, deixou as cinco cópias mais novas, a mais nova em `-1`.
pub fn back_up(path: &std::path::Path, max_backups: u32) -> std::io::Result<()> {
    let (Some(folder), Some(name)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Ok(());
    };
    for n in (1..=max_backups).rev() {
        let old = folder.join(backup_name(name, n));
        if old.exists() {
            if n >= max_backups {
                std::fs::remove_file(&old)?;
            } else {
                std::fs::rename(&old, folder.join(backup_name(name, n + 1)))?;
            }
        }
    }
    std::fs::copy(path, folder.join(backup_name(name, 1))).map(|_| ())
}

// ---------------------------------------------------------------------------------------------------------------------------
// O texto que o NightConfig escreve.

/// O nível de uma seção: a raiz é 0, `sounds` é 1 e `rendering.particle` é 2.
fn depth_of(name: &str) -> usize {
    if name.is_empty() {
        0
    } else {
        name.split('.').count()
    }
}

/// Um comentário recuado: cada linha do texto vira `#<linha>`.
fn comment_text(comment: &str, tabs: usize) -> String {
    let mut text = String::new();
    for line in comment.split('\n') {
        text.push_str(&format!("{}#{line}\n", "\t".repeat(tabs)));
    }
    text
}

/// As linhas de comentário de uma chave: o texto e, nos números de `defineInRange`, o padrão e a faixa (`ModConfigSpec.java:375-376`).
fn entry_comment(entry: &Entry) -> Vec<String> {
    let mut lines: Vec<String> = entry.comment.split('\n').map(str::to_owned).collect();
    if entry.setting.has_range() {
        lines.push(format!(
            " Default: {}",
            value_text(default_of(entry.setting))
        ));
        lines.push(format!(" Range: {}", range_text(entry.setting)));
    }
    lines
}

fn entry_text(entry: &Entry, value: Value, depth: usize) -> String {
    let tabs = "\t".repeat(depth);
    let mut text = String::new();
    for line in entry_comment(entry) {
        text.push_str(&format!("{tabs}#{line}\n"));
    }
    text.push_str(&format!("{tabs}{} = {}\n", entry.key, value_text(value)));
    text
}

/// `Range.toString` (`ModConfigSpec.java:1065`): `> mínimo` quando o máximo de um `int` é `Integer.MAX_VALUE`, `< máximo` quando o mínimo é o
/// `Integer.MIN_VALUE`, e `mínimo ~ máximo` nos demais.
fn range_text(setting: Setting) -> String {
    match setting {
        Setting::Int { min, max, .. } => {
            if max == i32::MAX {
                format!("> {min}")
            } else if min == i32::MIN {
                format!("< {max}")
            } else {
                format!("{min} ~ {max}")
            }
        }
        Setting::Long { min, max, .. } => format!("{min} ~ {max}"),
        Setting::Float { min, max, .. } => format!("{} ~ {}", java_double(min), java_double(max)),
        Setting::Bool { .. } | Setting::IntWhere { .. } => String::new(),
    }
}

/// `Double.toString`: decimal de 10⁻³ a 10⁷ (sempre com uma casa depois do ponto) e científico (`1.024E9`) fora disso. Os dígitos são os mais curtos que
/// leem de volta ao mesmo número, como o JDK desde o 19.
pub fn java_double(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // `{:e}` dá os dígitos mais curtos: `1.024e9`, `4e5`, `5e-1`.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("o formato científico tem `e`");
    let exponent: i32 = exponent.parse().expect("expoente inteiro");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if (1e-3..1e7).contains(&value.abs()) {
        let point = exponent + 1;
        let text = if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if (point as usize) >= digits.len() {
            format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        };
        format!("{sign}{text}")
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        format!("{sign}{first}.{rest}E{exponent}")
    }
}

fn default_of(setting: Setting) -> Value {
    match setting {
        Setting::Bool { default } => Value::Bool(default),
        Setting::Float { default, .. } => Value::Float(default),
        Setting::Int { default, .. } | Setting::IntWhere { default, .. } => Value::Int(default),
        Setting::Long { default, .. } => Value::Long(default),
    }
}

fn value_text(value: Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Float(value) => java_double(value),
        Value::Int(value) => value.to_string(),
        Value::Long(value) => value.to_string(),
    }
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

/// O nome da seção que um bloco já escrito abre (o primeiro cabeçalho que tem), ou vazio para a raiz.
fn block_name(block: &str) -> String {
    block.lines().find_map(header_of).unwrap_or_default()
}

/// `text` sem as seções `prefix` e `prefix.*` e com `new` no fim: quem escreve só as suas seções de um arquivo partilhado (as posições das janelas,
/// `window.<nome>`) não apaga as dos outros.
pub fn replace_sections(text: &str, prefix: &str, new: &str) -> String {
    let dotted = format!("{prefix}.");
    let mut blocks: Vec<String> = parse_chunks(text)
        .into_iter()
        .filter(|chunk| !(chunk.name == prefix || chunk.name.starts_with(&dotted)))
        .map(|chunk| chunk.lines.join("\n"))
        .collect();
    blocks.push(new.to_owned());
    join_blocks(blocks)
}

// ---------------------------------------------------------------------------------------------------------------------------
// A leitura: o subconjunto de TOML que uma configuração usa.

/// O que o arquivo diz de uma chave, antes de a especificação olhar.
#[derive(Clone, Debug, PartialEq)]
enum Raw {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

/// Uma chave do arquivo, com os comentários logo acima dela e as linhas como estão.
struct Found {
    key: String,
    value: Option<Raw>,
    comments: Vec<String>,
    lines: Vec<String>,
}

/// Um pedaço do arquivo: de um cabeçalho `[seção]` (com os comentários logo acima) até o próximo. O primeiro, sem cabeçalho, é a raiz.
struct Chunk {
    name: String,
    /// O texto de cada linha de comentário logo acima do cabeçalho, depois do `#`.
    header_comments: Vec<String>,
    lines: Vec<String>,
    entries: Vec<Found>,
}

fn header_of(line: &str) -> Option<String> {
    let inner = line.trim().strip_prefix('[')?;
    Some(inner.split(']').next()?.trim().to_owned())
}

fn comment_of(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    trimmed
        .strip_prefix('#')
        .map(|rest| rest.trim_end_matches('\r').to_owned())
}

fn parse_chunks(text: &str) -> Vec<Chunk> {
    let mut out = vec![Chunk {
        name: String::new(),
        header_comments: Vec::new(),
        lines: Vec::new(),
        entries: Vec::new(),
    }];
    // Os comentários seguidos, sem linha em branco no meio, e as linhas deles.
    let mut pending: Vec<String> = Vec::new();
    let mut pending_lines: Vec<String> = Vec::new();
    // Um valor de lista que o arquivo quebra em várias linhas: as seguintes são dele, não cabeçalho nem comentário.
    let mut open_list = 0_i32;
    for line in text.lines() {
        if open_list > 0 {
            open_list += bracket_balance(line);
            let current = out.last_mut().expect("sempre há um pedaço");
            current.lines.push(line.to_owned());
            if let Some(entry) = current.entries.last_mut() {
                entry.lines.push(line.to_owned());
            }
            continue;
        }
        if let Some(comment) = comment_of(line) {
            pending.push(comment);
            pending_lines.push(line.to_owned());
            continue;
        }
        if let Some(name) = header_of(line) {
            // Os comentários da seção ficam logo acima do cabeçalho e saem do pedaço anterior (nunca chegaram a entrar nele).
            let mut lines = std::mem::take(&mut pending_lines);
            lines.push(line.to_owned());
            out.push(Chunk {
                name,
                header_comments: std::mem::take(&mut pending),
                lines,
                entries: Vec::new(),
            });
            continue;
        }
        // Qualquer outra linha fecha o bloco de comentários: ele fica no pedaço atual, e uma chave leva os dele.
        let current = out.last_mut().expect("sempre há um pedaço");
        let comments = std::mem::take(&mut pending);
        let mut lines = std::mem::take(&mut pending_lines);
        if line.trim().is_empty() {
            current.lines.append(&mut lines);
            current.lines.push(line.to_owned());
            continue;
        }
        lines.push(line.to_owned());
        if let Some((key, value)) = parse_entry(line) {
            open_list = line
                .split_once('=')
                .map_or(0, |(_, value)| bracket_balance(value))
                .max(0);
            current.lines.extend(lines.iter().cloned());
            current.entries.push(Found {
                key,
                value,
                comments,
                lines,
            });
        } else {
            current.lines.extend(lines);
        }
    }
    // Comentários soltos no fim do arquivo.
    if !pending_lines.is_empty() {
        out.last_mut()
            .expect("sempre há um pedaço")
            .lines
            .append(&mut pending_lines);
    }
    out.retain(|chunk| chunk.lines.iter().any(|line| !line.trim().is_empty()));
    out
}

/// Quantos `[` ficam abertos no fim da linha, sem contar o que está entre aspas nem depois de um `#`.
fn bracket_balance(line: &str) -> i32 {
    let (mut depth, mut quote) = (0, None::<char>);
    for c in line.chars() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '#') => break,
            (None, '[') => depth += 1,
            (None, ']') => depth -= 1,
            (None, _) => {}
        }
    }
    depth
}

/// `chave = valor`. A chave com ponto, aspas ou outra forma é ignorada, e o texto de uma linha que não se entende também; um valor que não se
/// entende deixa a chave sem valor (o que a especificação corrige).
fn parse_entry(line: &str) -> Option<(String, Option<Raw>)> {
    let (key, value) = line.trim().split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((key.to_owned(), parse_value(value.trim())))
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
        "inf" | "+inf" => return Some(Raw::Float(f64::INFINITY)),
        "-inf" => return Some(Raw::Float(f64::NEG_INFINITY)),
        _ => {}
    }
    let digits: String = value.chars().filter(|c| *c != '_').collect();
    if let Ok(integer) = digits.parse::<i64>() {
        return Some(Raw::Int(integer));
    }
    // Um inteiro além do `long` não cabe em nada do Java; vale como número grande e vai ao limite.
    digits
        .parse::<f64>()
        .ok()
        .filter(|number| !number.is_nan())
        .map(Raw::Float)
}

/// `stringsMatchNormalizingNewLines`: dois comentários vazios são iguais, e fora isso vale o texto linha a linha.
fn comments_match(found: &[String], expected: &str) -> bool {
    let found = found.join("\n");
    if found.trim().is_empty() || expected.trim().is_empty() {
        return found.trim().is_empty() == expected.trim().is_empty();
    }
    found == expected
}

/// `ValueSpec.test` e `correct` de uma chave. Devolve o valor e se foi preciso corrigi-lo.
fn correct(setting: Setting, raw: Option<&Raw>, lookup: Lookup<'_>) -> (Value, bool) {
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
        Setting::Float { default, min, max } => match number_of(raw) {
            Some(number) if (min..=max).contains(&number) => (Value::Float(number), false),
            Some(number) if number < min => (Value::Float(min), true),
            Some(_) => (Value::Float(max), true),
            None => (Value::Float(default), true),
        },
        Setting::Int { default, min, max } => match raw {
            Some(Raw::Int(number)) if (i64::from(min)..=i64::from(max)).contains(number) => {
                (Value::Int(*number as i32), false)
            }
            Some(Raw::Int(number)) if *number < i64::from(min) => (Value::Int(min), true),
            Some(Raw::Int(_)) => (Value::Int(max), true),
            Some(Raw::Float(number)) if *number < f64::from(min) => (Value::Int(min), true),
            Some(Raw::Float(number)) if *number > f64::from(max) => (Value::Int(max), true),
            _ => (Value::Int(default), true),
        },
        Setting::Long { default, min, max } => match raw {
            Some(Raw::Int(number)) if (min..=max).contains(number) => (Value::Long(*number), false),
            Some(Raw::Int(number)) if *number < min => (Value::Long(min), true),
            Some(Raw::Int(_)) => (Value::Long(max), true),
            Some(Raw::Float(number)) if *number < min as f64 => (Value::Long(min), true),
            Some(Raw::Float(number)) if *number > max as f64 => (Value::Long(max), true),
            _ => (Value::Long(default), true),
        },
        Setting::IntWhere { default, valid } => match raw {
            Some(Raw::Int(number)) => match i32::try_from(*number) {
                Ok(number) if valid(number, lookup) => (Value::Int(number), false),
                _ => (Value::Int(default), true),
            },
            _ => (Value::Int(default), true),
        },
    }
}

fn number_of(raw: Option<&Raw>) -> Option<f64> {
    match raw? {
        Raw::Int(number) => Some(*number as f64),
        Raw::Float(number) => Some(*number),
        _ => None,
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
    }

    #[test]
    fn a_comment_after_the_value_and_unknown_keys_do_not_matter_for_the_values() {
        let loaded = SPEC.load("# a\n[sounds]\n\tbaseVolume = 2.5 # alto\n\tother = 1\n\tenableMachine = false # mudo\n");
        assert_eq!((volume(&loaded), machine(&loaded)), (2.5, false));
    }

    #[test]
    fn a_file_without_the_comments_is_written_again_with_them() {
        // `ModConfigSpec.isCorrect` conta o comentário que não é o da especificação (linha 296).
        let text = "[sounds]\n\tbaseVolume = 1.0\n\tenableMachine = true\n";
        let loaded = SPEC.load(text);
        assert!(loaded.corrected);
        let written = SPEC.rewrite(text, &loaded);
        assert_eq!(written, SPEC.default_text());
        assert!(!SPEC.load(&written).corrected);
        // Um comentário trocado também.
        let edited = SPEC
            .default_text()
            .replace("Play sounds for active machines.", "toca");
        assert!(SPEC.load(&edited).corrected);
    }

    #[test]
    fn the_rewritten_file_keeps_what_the_spec_does_not_know() {
        let text = "[sounds]\nbaseVolume = 99\nenableMachine = true\nenablePlayer = false\n[rendering]\nberRange = 4\n";
        let loaded = SPEC.load(text);
        assert!(loaded.corrected);
        let written = SPEC.rewrite(text, &loaded);
        assert!(written.contains("baseVolume = 10.0"));
        // O mod de verdade tem esta chave e esta seção: guardá-las deixa o arquivo andar entre os dois.
        assert!(written.contains("enablePlayer = false"));
        assert!(written.contains("[rendering]\nberRange = 4"));
        let again = SPEC.load(&written);
        assert!(!again.corrected, "{written}");
        assert_eq!(SPEC.rewrite(&written, &again), written);
    }

    #[test]
    fn a_list_the_file_breaks_in_lines_stays_with_its_key() {
        let text = SPEC.default_text()
            + "\tmodBlacklist = [\n\t\t\"a\",\n\t\t\"b\"\n\t]\n\n[tail]\n\tx = 1\n";
        let loaded = SPEC.load(&text);
        assert!(!loaded.corrected, "{text}");
        let again = SPEC.rewrite(
            &text.replace("baseVolume = 1.0", "baseVolume = 99"),
            &SPEC.load(&text.replace("baseVolume = 1.0", "baseVolume = 99")),
        );
        assert!(
            again.contains("modBlacklist = [\n\t\t\"a\",\n\t\t\"b\"\n\t]"),
            "{again}"
        );
        assert!(again.contains("[tail]\n\tx = 1"));
        assert!(again.contains("baseVolume = 10.0"));
    }

    #[test]
    fn an_unknown_key_or_section_is_not_an_error() {
        let text = SPEC.default_text() + "\tenablePlayer = true\n\n[rendering]\n\tberRange = 4\n";
        assert!(!SPEC.load(&text).corrected);
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
    fn rewriting_adds_a_missing_section_before_the_windows() {
        let text = "[window.upgrade]\nx = 1\ny = 2\npinned = true\n";
        let loaded = SPEC.load(text);
        assert!(loaded.corrected, "falta a seção sounds");
        let written = SPEC.rewrite(text, &loaded);
        // `sounds` vem antes de `window` no arquivo do original.
        assert_eq!(written, format!("{}\n{text}", SPEC.default_text()));
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

    // ---- Inteiros, longos, validadores, raiz e sub-seções ----

    fn at_most_tank(rate: i32, lookup: Lookup<'_>) -> bool {
        let Some(Value::Long(tank)) = lookup("gen", "tank") else {
            return false;
        };
        rate > 0 && i64::from(rate) <= tank
    }

    const SERVER: Spec = Spec {
        sections: &[
            Section {
                name: "",
                comment: "",
                entries: &[Entry {
                    key: "delay",
                    comment: "Ticks.",
                    setting: Setting::Int {
                        default: 60,
                        min: 0,
                        max: 1200,
                    },
                }],
            },
            Section {
                name: "gen",
                comment: "Generator",
                entries: &[
                    Entry {
                        key: "any",
                        comment: "Any.",
                        setting: Setting::Int {
                            default: 7,
                            min: 0,
                            max: i32::MAX,
                        },
                    },
                    Entry {
                        key: "tank",
                        comment: "Tank.",
                        setting: Setting::Long {
                            default: 1000,
                            min: 1,
                            max: i64::MAX,
                        },
                    },
                    Entry {
                        key: "rate",
                        comment: "Rate.",
                        setting: Setting::IntWhere {
                            default: 100,
                            valid: at_most_tank,
                        },
                    },
                ],
            },
            Section {
                name: "gen.deep",
                comment: "Deeper",
                entries: &[Entry {
                    key: "scale",
                    comment: "Scale.",
                    setting: Setting::Float {
                        default: 400.0,
                        min: 0.1,
                        max: 4_000_000.0,
                    },
                }],
            },
        ],
    };

    #[test]
    fn the_server_file_has_the_root_keys_first_and_nested_sections_indented() {
        assert_eq!(
            SERVER.default_text(),
            "#Ticks.\n# Default: 60\n# Range: 0 ~ 1200\ndelay = 60\n\n#Generator\n[gen]\n\t#Any.\n\t# Default: 7\n\t# Range: > 0\n\tany = 7\n\t#Tank.\n\t# Default: 1000\n\t# Range: 1 ~ 9223372036854775807\n\ttank = 1000\n\t#Rate.\n\trate = 100\n\n\t#Deeper\n\t[gen.deep]\n\t\t#Scale.\n\t\t# Default: 400.0\n\t\t# Range: 0.1 ~ 4000000.0\n\t\tscale = 400.0\n"
        );
        let loaded = SERVER.load(&SERVER.default_text());
        assert!(!loaded.corrected);
        assert_eq!(loaded, SERVER.defaults());
    }

    #[test]
    fn integers_and_longs_go_to_the_limit_and_a_wrong_type_goes_to_the_default() {
        let load = |text: &str| SERVER.load(text);
        let loaded = load("delay = 5000\n[gen]\n any = -4\n tank = 0\n");
        assert_eq!(loaded.int(&SERVER, "", "delay"), Some(1200));
        assert_eq!(loaded.int(&SERVER, "gen", "any"), Some(0));
        assert_eq!(loaded.long(&SERVER, "gen", "tank"), Some(1));
        // Um long de `int` passa do `Integer.MAX_VALUE` e vai ao limite; o inteiro de um `long` cabe.
        let loaded = load("delay = 99999999999\n[gen]\n any = 99999999999\n tank = 5000000000\n");
        assert_eq!(loaded.int(&SERVER, "", "delay"), Some(1200));
        assert_eq!(loaded.int(&SERVER, "gen", "any"), Some(i32::MAX));
        assert_eq!(loaded.long(&SERVER, "gen", "tank"), Some(5_000_000_000));
        let loaded = load("delay = \"x\"\n[gen]\n any = true\n tank = \"lots\"\n");
        assert_eq!(loaded.int(&SERVER, "", "delay"), Some(60));
        assert_eq!(loaded.int(&SERVER, "gen", "any"), Some(7));
        assert_eq!(loaded.long(&SERVER, "gen", "tank"), Some(1000));
        // O limite de um `long` que o arquivo escreve além do que cabe vale o máximo.
        let loaded = load("[gen]\n tank = 99999999999999999999\n");
        assert_eq!(loaded.long(&SERVER, "gen", "tank"), Some(i64::MAX));
        // Um número com ponto numa chave de `int` não existe no original (o Java falha ao ler): padrão.
        let loaded = load("delay = 3.5\n");
        assert_eq!(loaded.int(&SERVER, "", "delay"), Some(60));
        assert!(loaded.corrected);
        // Fora do limite, um número com ponto vai ao limite, como no original.
        assert_eq!(
            load("delay = 5000.5\n").int(&SERVER, "", "delay"),
            Some(1200)
        );
    }

    #[test]
    fn a_validator_without_a_range_sends_the_value_to_the_default() {
        let load = |text: &str| SERVER.load(text).int(&SERVER, "gen", "rate");
        assert_eq!(load("[gen]\n rate = 100\n"), Some(100));
        assert_eq!(load("[gen]\n rate = 1000\n"), Some(1000));
        // Passa do tanque (1000, o padrão, num arquivo ainda não lido): o padrão, não o limite.
        assert_eq!(load("[gen]\n rate = 1001\n"), Some(100));
        assert_eq!(load("[gen]\n rate = 0\n"), Some(100));
        assert_eq!(load("[gen]\n rate = -3\n"), Some(100));
        assert_eq!(load("[gen]\n rate = \"x\"\n"), Some(100));
        // Um `long` de fora do `int` não é um `Integer`.
        assert_eq!(load("[gen]\n rate = 5000000000\n"), Some(100));
        assert!(SERVER.load("[gen]\n rate = 1001\n").corrected);
    }

    #[test]
    fn a_validator_that_looks_at_another_key_sees_what_was_loaded_before() {
        let before = SERVER.load("[gen]\n tank = 5000\n");
        // Num arquivo ainda não lido o validador vê o tanque padrão (1000): o original devolveu 100 a um 3000 com tanque de 5000 na primeira leitura.
        let first = SERVER.load("[gen]\n tank = 5000\n rate = 3000\n");
        assert_eq!(first.int(&SERVER, "gen", "rate"), Some(100));
        // Numa leitura de novo, com o tanque de 5000 em cache, o 3000 vale.
        let again = SERVER.load_after("[gen]\n tank = 5000\n rate = 3000\n", Some(&before));
        assert_eq!(again.int(&SERVER, "gen", "rate"), Some(3000));
    }

    #[test]
    fn a_file_missing_a_key_or_a_section_is_written_again_in_place() {
        let text = "delay = 60\n\n[gen]\n\tany = 7\n";
        let loaded = SERVER.load(text);
        assert!(loaded.corrected);
        let written = SERVER.rewrite(text, &loaded);
        assert_eq!(written, SERVER.default_text());
    }

    #[test]
    fn the_double_text_is_the_one_java_writes() {
        assert_eq!(java_double(1.0), "1.0");
        assert_eq!(java_double(400.0), "400.0");
        assert_eq!(java_double(0.6), "0.6");
        assert_eq!(java_double(0.1), "0.1");
        assert_eq!(java_double(0.001), "0.001");
        assert_eq!(java_double(4_000_000.0), "4000000.0");
        assert_eq!(java_double(9_999_999.0), "9999999.0");
        assert_eq!(java_double(10_000_000.0), "1.0E7");
        assert_eq!(java_double(1_024_000_000.0), "1.024E9");
        assert_eq!(java_double(0.0001), "1.0E-4");
        assert_eq!(java_double(0.0), "0.0");
        assert_eq!(java_double(f64::from(0.4_f32)), "0.4000000059604645");
        assert_eq!(java_double(-2.5), "-2.5");
        assert_eq!(java_double(0.25), "0.25");
        assert_eq!(java_double(123.456), "123.456");
    }
}
