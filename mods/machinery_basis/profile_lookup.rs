// A consulta de um nome de jogador aos serviços da Mojang, como o authlib 10.0.77 do original faz para o `nameToIdCache` do servidor
// (`MinecraftServicesProfileRepository.findProfileByName`, `MinecraftClient`, `MinecraftServicesDiscoveryService`, `RetryableFetch`).
// Só `std`, `ureq` e `serde_json`, que o `horizon-client` já tem: o arquivo é um módulo do jogo e também é compilado sozinho pelo teste
// `core/tests/profile_lookup.rs`, contra um servidor HTTP local.
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// `MinecraftServicesEnvironment.PROD`: o documento de descoberta dos serviços.
pub const DISCOVERY_URL: &str = "https://discovery.minecraftservices.com/minecraft/client";
/// `MinecraftClient.CONNECT_TIMEOUT_MS`.
pub const CONNECT_TIMEOUT_MS: u64 = 5000;
/// `MinecraftClient.READ_TIMEOUT_MS`: vale para cada leitura, não para a resposta inteira (`HttpURLConnection.setReadTimeout`).
pub const READ_TIMEOUT_MS: u64 = 5000;
/// `RetryableFetch.fetch(..., 24, 1)` em `createDiscoverySupplier`: o documento é pedido de novo a cada 24 horas.
pub const REFRESH_INTERVAL_HOURS: u64 = 24;
/// O intervalo depois de uma falha, em minutos, dobrado a cada falha seguida.
pub const RETRY_INTERVAL_MINUTES: u64 = 1;
/// `RetryableFetch.MAX_BACKOFF_EXPONENT`: o intervalo para de dobrar em 64 minutos.
pub const MAX_BACKOFF_EXPONENT: u32 = 6;
/// `http.maxRedirects` do `HttpURLConnection`.
const MAX_REDIRECTS: u32 = 20;

/// Os tempos do authlib. O teste encolhe o minuto e a hora para ver o agendamento.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub connect: Duration,
    pub read: Duration,
    pub minute: Duration,
    pub hour: Duration,
}

impl Timing {
    pub const AUTHLIB: Timing = Timing {
        connect: Duration::from_millis(CONNECT_TIMEOUT_MS),
        read: Duration::from_millis(READ_TIMEOUT_MS),
        minute: Duration::from_secs(60),
        hour: Duration::from_secs(3600),
    };
}

/// `MinecraftClientException`: o serviço não respondeu, respondeu com erro (400 ou mais) ou mandou um JSON que não se lê.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientError {
    ServiceUnavailable(String),
    Http(u16),
    Json(String),
}

/// `MinecraftClient.get(url, classe)` até o texto: `Ok(None)` é o corpo vazio (o 204 da Mojang para um nome que não existe, e o 304); todo status de 400 para cima
/// é erro, e também a conexão que não abre e a leitura que passa do tempo. Sem `Authorization` (`MinecraftClient.unauthenticated`) e sem nova tentativa.
pub fn http_get(url: &str, timing: Timing) -> Result<Option<String>, ClientError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(timing.connect)
        .timeout_read(timing.read)
        .redirects(MAX_REDIRECTS)
        .build();
    match agent.get(url).call() {
        Ok(response) => {
            // O 304, que o authlib devolve como "nada" sem ler o corpo, não tem corpo: cai no corpo vazio.
            let body = response.into_string().map_err(|error| {
                ClientError::ServiceUnavailable(format!("Failed to read from {url} due to {error}"))
            })?;
            Ok((!body.is_empty()).then_some(body))
        }
        Err(ureq::Error::Status(status, _)) => Err(ClientError::Http(status)),
        Err(ureq::Error::Transport(error)) => Err(ClientError::ServiceUnavailable(format!(
            "Failed to read from {url} due to {error}"
        ))),
    }
}

/// `UndashedUuid.fromStringLenient`: os primeiros 32 caracteres de palavra ganham os traços, e o resultado passa pelo `UUID.fromString` do Java (cinco grupos
/// hexadecimais, cada um cortado na largura do campo). O texto que o Java recusaria com uma exceção não tratada dá `None` aqui.
pub fn undashed_uuid(text: &str) -> Option<u128> {
    let word = |unit: char| unit.is_ascii_alphanumeric() || unit == '_';
    let chars: Vec<char> = text.chars().collect();
    let run = (0..chars.len()).find(|start| {
        chars.len() - start >= 32 && chars[*start..*start + 32].iter().all(|unit| word(*unit))
    });
    let dashed: String = match run {
        Some(start) => {
            let mut out: String = chars[..start].iter().collect();
            for (index, unit) in chars[start..start + 32].iter().enumerate() {
                if matches!(index, 8 | 12 | 16 | 20) {
                    out.push('-');
                }
                out.push(*unit);
            }
            out.extend(&chars[start + 32..]);
            out
        }
        None => text.to_owned(),
    };
    let parts: Vec<&str> = dashed.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let mut fields = [0u128; 5];
    for (field, part) in fields.iter_mut().zip(&parts) {
        if part.is_empty() || part.len() > 16 || !part.chars().all(|unit| unit.is_ascii_hexdigit())
        {
            return None;
        }
        *field = u128::from(u64::from_str_radix(part, 16).ok()?);
    }
    Some(
        ((fields[0] & 0xffff_ffff) << 96)
            | ((fields[1] & 0xffff) << 80)
            | ((fields[2] & 0xffff) << 64)
            | ((fields[3] & 0xffff) << 48)
            | (fields[4] & 0xffff_ffff_ffff),
    )
}

/// A resposta do `getByName` lida como o `NameAndId` do authlib (`{"id": sem traços, "name"}`). `Ok(None)` é o `null` do JSON; o JSON que não se lê é o
/// `JSON_ERROR`. Um perfil sem `id` ou sem `name` (que no original estoura mais adiante, fora do `catch`) também é erro aqui.
pub fn parse_profile(body: &str) -> Result<Option<(u128, String)>, ClientError> {
    let failed = || ClientError::Json(format!("Failed to read value {body}"));
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| failed())?;
    if value.is_null() {
        return Ok(None);
    }
    let id = value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .and_then(undashed_uuid)
        .ok_or_else(failed)?;
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(failed)?;
    Ok(Some((id, name.to_owned())))
}

/// `MinecraftServicesDiscoveryService.getUrl(serviço, chave)` sobre o documento: o documento `offline` (o que vale enquanto o pedido não deu certo) e a chave que
/// falta são `SERVICE_UNAVAILABLE`.
pub fn endpoint_uri(
    document: Option<&serde_json::Value>,
    service: &str,
    key: &str,
) -> Result<String, ClientError> {
    let unavailable = || ClientError::ServiceUnavailable("Services are unavailable".to_owned());
    let document = document.ok_or_else(unavailable)?;
    if document
        .get("environment")
        .and_then(serde_json::Value::as_str)
        == Some("offline")
    {
        return Err(unavailable());
    }
    document
        .get("discovery")
        .and_then(|discovery| discovery.get(service))
        .and_then(|endpoints| endpoints.get("endpoints"))
        .and_then(|endpoints| endpoints.get(key))
        .and_then(|endpoint| endpoint.get("uri"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(unavailable)
}

struct Fetched {
    /// `ready`: a primeira tentativa acabou, com ou sem resposta.
    ready: bool,
    /// `value`: o último documento lido; uma falha depois dele não o apaga.
    value: Option<serde_json::Value>,
    /// Quantas tentativas a thread já fez (para o teste ver o agendamento).
    attempts: u32,
}

/// O `MinecraftServicesDiscoveryService` com o repositório de perfis: o documento de descoberta buscado numa thread própria desde a criação
/// (`RetryableFetch`, na thread `Minecraft Services Discovery`) e a consulta de um nome, que roda na thread de quem chama.
pub struct Services {
    fetched: Arc<(Mutex<Fetched>, Condvar)>,
    timing: Timing,
}

impl Services {
    /// `MinecraftServicesDiscoveryService.create`: a busca do documento começa agora e se repete a cada 24 horas; enquanto nenhuma deu certo, a nova tentativa vem
    /// em 1, 2, 4, 8, 16, 32 e 64 minutos, e daí de 64 em 64.
    pub fn start(discovery_url: String, timing: Timing) -> Services {
        let fetched = Arc::new((
            Mutex::new(Fetched {
                ready: false,
                value: None,
                attempts: 0,
            }),
            Condvar::new(),
        ));
        let shared = Arc::clone(&fetched);
        let spawned = std::thread::Builder::new()
            .name("Minecraft Services Discovery".to_owned())
            .spawn(move || {
                let mut failure_count: u32 = 0;
                loop {
                    let answer = http_get(&discovery_url, timing).and_then(|body| {
                        body.map(|body| {
                            serde_json::from_str::<serde_json::Value>(&body)
                                .map_err(|error| ClientError::Json(error.to_string()))
                        })
                        .transpose()
                    });
                    let has_value = {
                        let (lock, ready) = &*shared;
                        let mut state = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        match answer {
                            Ok(Some(document)) => {
                                state.value = Some(document);
                                failure_count = 0;
                            }
                            Ok(None) => {}
                            Err(error) => {
                                eprintln!("machinery_basis: Failed to connect: {error:?}")
                            }
                        }
                        state.ready = true;
                        state.attempts += 1;
                        ready.notify_all();
                        state.value.is_some()
                    };
                    let wait = if has_value {
                        timing.hour * (REFRESH_INTERVAL_HOURS as u32)
                    } else {
                        let exponent = failure_count.min(MAX_BACKOFF_EXPONENT);
                        failure_count += 1;
                        timing.minute * ((RETRY_INTERVAL_MINUTES as u32) << exponent)
                    };
                    std::thread::sleep(wait);
                }
            });
        if spawned.is_err() {
            // Sem a thread nada ficaria pronto: vale o documento `offline`.
            let (lock, ready) = &*fetched;
            lock.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .ready = true;
            ready.notify_all();
        }
        Services { fetched, timing }
    }

    /// O `Supplier` do `RetryableFetch`: espera a primeira tentativa acabar (`ready.join()`) e devolve o que houver.
    fn document(&self) -> Option<serde_json::Value> {
        let (lock, ready) = &*self.fetched;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !state.ready {
            state = ready
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        state.value.clone()
    }

    /// Quantas vezes o documento já foi pedido.
    pub fn attempts(&self) -> u32 {
        self.fetched
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .attempts
    }

    /// `MinecraftServicesProfileRepository.findProfileByName` sem o `catch`: o GET no `getByName` com o nome em minúsculas no lugar de `{name}`.
    pub fn try_find_profile_by_name(
        &self,
        name: &str,
    ) -> Result<Option<(u128, String)>, ClientError> {
        let uri = endpoint_uri(self.document().as_ref(), "profiles", "getByName")?;
        match http_get(&uri.replace("{name}", &name.to_lowercase()), self.timing)? {
            Some(body) => parse_profile(&body),
            None => Ok(None),
        }
    }

    /// `findProfileByName`: todo erro vira "não achei", com o aviso no log.
    pub fn find_profile_by_name(&self, name: &str) -> Option<(u128, String)> {
        self.try_find_profile_by_name(name).unwrap_or_else(|error| {
            eprintln!("machinery_basis: Couldn't find profile with name: {name}: {error:?}");
            None
        })
    }
}

static SERVICES: OnceLock<Services> = OnceLock::new();

/// Os serviços do jogo, criados na primeira chamada. `HORIZON_API_DISCOVERY` troca o endereço do documento, como no próprio Horizon
/// (`horizon_server_files::profiles::discovery_url`, o `-Dminecraft.api.discovery.host` do authlib).
pub fn services() -> &'static Services {
    SERVICES.get_or_init(|| {
        let url = std::env::var("HORIZON_API_DISCOVERY")
            .ok()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| DISCOVERY_URL.to_owned());
        Services::start(url, Timing::AUTHLIB)
    })
}
