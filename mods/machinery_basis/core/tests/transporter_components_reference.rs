//! Os componentes do item numa pilha do transportador contra o original (frente `transportador-ajustes`, Fase 3). `dev/referencia/rastros/transporter_components.jsonl` vem de
//! `TransporterAjustesTraceTest.components`: para cada pilha (uma ferramenta gasta e nomeada, um item nomeado com aspas no nome, um item do Mekanism com energia e um item sem
//! componentes), o `ItemResource.CODEC` como SNBT, o que `TransporterStack.write` e `writeToUpdateTag` gravam e se `read` e `readFromUpdate` devolvem o mesmo recurso e a mesma quantidade.
//! A lib não conhece os componentes de um item (`ItemResource.components` guarda cada um como texto): o teste faz o papel de quem liga o jogo, com um codec que lê e escreve o texto como
//! SNBT.
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::transporter::nbt::{Compound, Nbt, to_snbt};
use machinery_basis_core::transporter::stack::TransporterStack;
use serde_json::Value;

/// O SNBT que o rastro tem (`CompoundTag.toString`): compostos, textos entre aspas duplas ou simples, números com ou sem sufixo.
struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> u8 {
        self.text[self.at]
    }

    fn eat(&mut self, expected: u8) {
        assert_eq!(self.peek(), expected, "em {}", self.at);
        self.at += 1;
    }

    fn value(&mut self) -> Nbt {
        match self.peek() {
            b'{' => {
                self.eat(b'{');
                let mut compound = Compound::new();
                while self.peek() != b'}' {
                    let key = self.key();
                    self.eat(b':');
                    compound.insert(key, self.value());
                    if self.peek() == b',' {
                        self.eat(b',');
                    }
                }
                self.eat(b'}');
                Nbt::Compound(compound)
            }
            b'"' | b'\'' => Nbt::String(self.quoted()),
            _ => self.number(),
        }
    }

    fn key(&mut self) -> String {
        if matches!(self.peek(), b'"' | b'\'') {
            return self.quoted();
        }
        let start = self.at;
        while self.peek() != b':' {
            self.at += 1;
        }
        String::from_utf8(self.text[start..self.at].to_vec()).unwrap()
    }

    fn quoted(&mut self) -> String {
        let quote = self.peek();
        self.at += 1;
        let mut out = Vec::new();
        while self.peek() != quote {
            if self.peek() == b'\\' {
                self.at += 1;
            }
            out.push(self.peek());
            self.at += 1;
        }
        self.at += 1;
        String::from_utf8(out).unwrap()
    }

    fn number(&mut self) -> Nbt {
        let start = self.at;
        while self.at < self.text.len() && !matches!(self.peek(), b',' | b'}') {
            self.at += 1;
        }
        let word = std::str::from_utf8(&self.text[start..self.at]).unwrap();
        match word.as_bytes()[word.len() - 1] {
            b'L' => Nbt::Long(word[..word.len() - 1].parse().unwrap()),
            b'b' => Nbt::Byte(word[..word.len() - 1].parse().unwrap()),
            b's' => Nbt::Short(word[..word.len() - 1].parse().unwrap()),
            _ => Nbt::Int(word.parse().unwrap()),
        }
    }
}

fn parse(text: &str) -> Nbt {
    let mut parser = Parser {
        text: text.as_bytes(),
        at: 0,
    };
    let value = parser.value();
    assert_eq!(parser.at, text.len(), "sobrou texto em {text}");
    value
}

/// `ItemResource.CODEC` ao ler: o `id` e os `components`, cada um guardado como o SNBT do valor.
fn decode(tag: &Compound) -> Option<ItemResource> {
    let id = tag.get("id")?.as_str()?;
    let texts: Vec<(String, String)> = tag
        .get("components")
        .and_then(Nbt::as_compound)
        .map(|components| {
            components
                .iter()
                .map(|(kind, value)| (kind.clone(), to_snbt(value)))
                .collect()
        })
        .unwrap_or_default();
    Some(ItemResource::with_components(
        id,
        64,
        Components::new(
            texts
                .iter()
                .map(|(kind, value)| (kind.as_str(), value.as_str())),
        ),
    ))
}

/// `ItemResource.CODEC` ao escrever: o `components` só existe quando o recurso tem algum (`optionalFieldOf`).
fn encode(item: &ItemResource) -> Compound {
    let mut resource = Compound::new();
    resource.insert("id".into(), Nbt::String(item.id.to_string()));
    if !item.components.is_empty() {
        let components: Compound = item
            .components
            .iter()
            .map(|(kind, value)| (kind.to_owned(), parse(value)))
            .collect();
        resource.insert("components".into(), Nbt::Compound(components));
    }
    resource
}

fn rows() -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/transporter_components.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// O `original_location` do `write` do rastro: o rastro grava a posição absoluta do bloco de cada execução.
fn original_location(write: &str) -> i64 {
    let Nbt::Compound(tag) = parse(write) else {
        panic!("{write}")
    };
    tag["original_location"].as_long().unwrap()
}

fn stack_of(row: &Value, resource: ItemResource) -> TransporterStack {
    let mut stack = TransporterStack::new();
    stack.progress = 37;
    stack.original_location = original_location(row["write"].as_str().unwrap());
    stack.set_stack(Some(&resource), row["n"].as_u64().unwrap() as u32);
    stack
}

fn resource_of(row: &Value) -> ItemResource {
    let Nbt::Compound(tag) = parse(row["res"].as_str().unwrap()) else {
        panic!()
    };
    decode(&tag).unwrap()
}

#[test]
fn the_reference_has_every_kind_of_item() {
    let rows = rows();
    assert_eq!(rows.len(), 4);
    let with_components = rows
        .iter()
        .filter(|row| row["res"].as_str().unwrap().contains("components"))
        .count();
    assert_eq!(with_components, 3);
}

/// `TransporterStack.write`: o `type` leva o `ItemResource.CODEC` inteiro, com os componentes.
#[test]
fn the_saved_stack_carries_the_components_like_the_original() {
    for row in rows() {
        let stack = stack_of(&row, resource_of(&row));
        let written = stack.write_with(&encode);
        assert_eq!(
            to_snbt(&Nbt::Compound(written)),
            row["write"].as_str().unwrap(),
            "a pilha {}",
            row["i"]
        );
    }
}

/// `writeToUpdateTag`: o mesmo item, o `path_type` e o anterior.
#[test]
fn the_update_tag_carries_the_components_like_the_original() {
    for row in rows() {
        let stack = stack_of(&row, resource_of(&row));
        let written = stack.write_to_update_tag_with(0, &encode);
        assert_eq!(
            to_snbt(&Nbt::Compound(written)),
            row["upd"].as_str().unwrap(),
            "a pilha {}",
            row["i"]
        );
    }
}

/// `read` e `readFromUpdate` devolvem o mesmo recurso (os componentes inclusive) e a mesma quantidade.
#[test]
fn the_stack_reads_back_with_its_components() {
    for row in rows() {
        let resource = resource_of(&row);
        let Nbt::Compound(written) = parse(row["write"].as_str().unwrap()) else {
            panic!()
        };
        let Nbt::Compound(updated) = parse(row["upd"].as_str().unwrap()) else {
            panic!()
        };
        assert!(
            row["back"].as_bool().unwrap(),
            "o original leu a pilha de volta"
        );
        for back in [
            TransporterStack::read_with(&written, &decode),
            TransporterStack::read_from_update_with(&updated, &decode),
        ] {
            assert_eq!(
                back.item_type(),
                Some(&resource),
                "o recurso da pilha {}",
                row["i"]
            );
            assert_eq!(back.size(), row["n"].as_u64().unwrap() as u32);
        }
    }
}

/// Sem o codec dos componentes (só o id) o recurso volta sem eles: o que o port fazia.
#[test]
fn a_plain_codec_loses_the_components() {
    let row = rows().remove(0);
    let stack = stack_of(&row, resource_of(&row));
    let plain = stack.write();
    assert_ne!(
        to_snbt(&Nbt::Compound(plain)),
        row["write"].as_str().unwrap()
    );
}
