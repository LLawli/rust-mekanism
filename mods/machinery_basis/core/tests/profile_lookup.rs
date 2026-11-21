//! A consulta de um nome à Mojang (`mods/machinery_basis/profile_lookup.rs`, o módulo do jogo) contra um servidor HTTP local que responde como os serviços:
//! o documento de descoberta e o `getByName`. O que se espera de cada resposta foi lido do authlib 10.0.77 do original
//! (`MinecraftServicesProfileRepository.findProfileByName`, `MinecraftClient.readServiceResponse`, `RetryableFetch`) e, para o 200 e o 204, visto nos serviços
//! de verdade em 2026-11-21 (`Notch` e um nome que não existe).

#[path = "../../profile_lookup.rs"]
#[allow(dead_code)]
mod profile_lookup;

use profile_lookup::{ClientError, Services, Timing};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// O que o servidor de teste faz com um pedido.
#[derive(Clone)]
enum Answer {
    /// Status, cabeçalhos a mais e corpo.
    Send(u16, Vec<(&'static str, String)>, String),
    /// Espera e responde.
    Late(Duration, Box<Answer>),
    /// Aceita a conexão e nunca responde.
    Silent,
    /// O corpo em pedaços, com uma pausa entre eles.
    Drip(Duration, Vec<String>),
}

#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
}

type Route = Arc<dyn Fn(&Request, usize) -> Answer + Send + Sync>;

struct Server {
    base: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Server {
    /// Sobe em `127.0.0.1` numa porta livre; `route` recebe o pedido e quantos pedidos vieram antes para o mesmo caminho.
    fn start(route: impl Fn(&Request, usize) -> Answer + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests: Arc<Mutex<Vec<Request>>> = Arc::new(Mutex::new(Vec::new()));
        let route: Route = Arc::new(route);
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (route, seen) = (Arc::clone(&route), Arc::clone(&seen));
                std::thread::spawn(move || serve(stream, &route, &seen));
            }
        });
        Server { base, requests }
    }

    fn paths(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.path.clone())
            .collect()
    }

    fn count(&self, path: &str) -> usize {
        self.paths().iter().filter(|seen| *seen == path).count()
    }
}

fn serve(stream: TcpStream, route: &Route, seen: &Mutex<Vec<Request>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (
        parts.next().unwrap_or("").to_owned(),
        parts.next().unwrap_or("").to_owned(),
    );
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
        }
    }
    let request = Request {
        method,
        path,
        headers,
    };
    let before = {
        let mut seen = seen.lock().unwrap();
        let before = seen
            .iter()
            .filter(|earlier| earlier.path == request.path)
            .count();
        seen.push(request.clone());
        before
    };
    answer(stream, route(&request, before));
}

fn answer(mut stream: TcpStream, answer: Answer) {
    match answer {
        Answer::Send(status, headers, body) => {
            let mut head = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
            let _ = stream.write_all(format!("{head}\r\n{body}").as_bytes());
        }
        Answer::Late(wait, then) => {
            std::thread::sleep(wait);
            self::answer(stream, *then);
        }
        Answer::Silent => std::thread::sleep(Duration::from_secs(30)),
        Answer::Drip(pause, pieces) => {
            let total: usize = pieces.iter().map(String::len).sum();
            let _ = stream.write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            );
            for piece in pieces {
                std::thread::sleep(pause);
                let _ = stream.write_all(piece.as_bytes());
                let _ = stream.flush();
            }
        }
    }
}

const NOTCH: &str =
    "{\n  \"id\" : \"069a79f444e94726a5befca90e38aaf5\",\n  \"name\" : \"Notch\"\n}";
const NOTCH_ID: u128 = 0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5;
const PROFILE_PREFIX: &str = "/users/profiles/minecraft/";

/// O documento de descoberta com a forma do de verdade, apontando o `getByName` para `base`.
fn discovery(base: &str) -> String {
    format!(
        r#"{{"environment":"prod","product":"minecraft","discovery":{{"profiles":{{"endpoints":{{"getManyByName":{{"uri":"{base}/profiles/minecraft"}},"getByName":{{"uri":"{base}{PROFILE_PREFIX}{{name}}"}},"getTexture":{{"validUris":["x"]}}}}}}}}}}"#
    )
}

fn ok(body: impl Into<String>) -> Answer {
    Answer::Send(200, Vec::new(), body.into())
}

/// Um servidor com a descoberta boa e `profile` respondendo o `getByName`.
fn mojang(profile: impl Fn(&str, usize) -> Answer + Send + Sync + 'static) -> Server {
    let base: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let known = Arc::clone(&base);
    let server =
        Server::start(
            move |request, before| match request.path.strip_prefix(PROFILE_PREFIX) {
                Some(name) => profile(name, before),
                None if request.path == "/discovery" => ok(discovery(&known.lock().unwrap())),
                None => Answer::Send(404, Vec::new(), String::new()),
            },
        );
    *base.lock().unwrap() = server.base.clone();
    server
}

fn short() -> Timing {
    Timing {
        connect: Duration::from_millis(400),
        read: Duration::from_millis(400),
        minute: Duration::from_millis(100),
        hour: Duration::from_secs(3600),
    }
}

fn services(server: &Server, timing: Timing) -> Services {
    Services::start(format!("{}/discovery", server.base), timing)
}

#[test]
fn the_constants_are_the_ones_of_authlib() {
    assert_eq!(
        profile_lookup::DISCOVERY_URL,
        "https://discovery.minecraftservices.com/minecraft/client"
    );
    assert_eq!(profile_lookup::CONNECT_TIMEOUT_MS, 5000);
    assert_eq!(profile_lookup::READ_TIMEOUT_MS, 5000);
    assert_eq!(profile_lookup::REFRESH_INTERVAL_HOURS, 24);
    assert_eq!(profile_lookup::RETRY_INTERVAL_MINUTES, 1);
    assert_eq!(profile_lookup::MAX_BACKOFF_EXPONENT, 6);
    assert_eq!(Timing::AUTHLIB.connect, Duration::from_secs(5));
    assert_eq!(Timing::AUTHLIB.read, Duration::from_secs(5));
    assert_eq!(Timing::AUTHLIB.minute, Duration::from_secs(60));
    assert_eq!(Timing::AUTHLIB.hour, Duration::from_secs(3600));
}

#[test]
fn a_known_name_is_one_get_with_the_name_in_lower_case() {
    let server = mojang(|name, _| {
        if name == "notch" {
            ok(NOTCH)
        } else {
            Answer::Send(404, Vec::new(), String::new())
        }
    });
    let services = services(&server, short());
    // O nome volta como a Mojang o escreve, não como foi pedido.
    assert_eq!(
        services.find_profile_by_name("NoTcH"),
        Some((NOTCH_ID, "Notch".to_owned()))
    );
    assert_eq!(
        server.paths(),
        vec!["/discovery".to_owned(), format!("{PROFILE_PREFIX}notch")]
    );
    let requests = server.requests.lock().unwrap().clone();
    let profile = &requests[1];
    assert_eq!(profile.method, "GET");
    // `MinecraftClient.unauthenticated`: sem `Authorization`; o GET não tem corpo.
    assert!(
        profile
            .headers
            .iter()
            .all(|(name, _)| name != "authorization"
                && name != "content-length"
                && name != "content-type"),
        "{:?}",
        profile.headers
    );
    // O documento de descoberta é pedido uma vez, não a cada consulta.
    assert_eq!(
        services.find_profile_by_name("notch"),
        Some((NOTCH_ID, "Notch".to_owned()))
    );
    assert_eq!(server.count("/discovery"), 1);
}

#[test]
fn no_content_not_found_and_too_many_requests_find_nobody_and_are_not_retried() {
    let server = mojang(|name, _| match name {
        "vazio" => Answer::Send(204, Vec::new(), String::new()),
        "sumiu" => Answer::Send(
            404,
            vec![("Content-Type", "application/json".to_owned())],
            r#"{"path":"/x","errorMessage":"Couldn't find any profile with name sumiu"}"#
                .to_owned(),
        ),
        "muitos" => Answer::Send(429, vec![("Retry-After", "1".to_owned())], String::new()),
        "caiu" => Answer::Send(
            500,
            vec![("Content-Type", "text/html".to_owned())],
            "<html>".to_owned(),
        ),
        "igual" => Answer::Send(304, Vec::new(), String::new()),
        "nulo" => ok("null"),
        "torto" => ok("{\"id\":"),
        "semid" => ok(r#"{"name":"SemId"}"#),
        _ => ok(NOTCH),
    });
    let services = services(&server, short());
    assert_eq!(services.try_find_profile_by_name("vazio"), Ok(None));
    assert_eq!(
        services.try_find_profile_by_name("sumiu"),
        Err(ClientError::Http(404))
    );
    assert_eq!(
        services.try_find_profile_by_name("muitos"),
        Err(ClientError::Http(429))
    );
    assert_eq!(
        services.try_find_profile_by_name("caiu"),
        Err(ClientError::Http(500))
    );
    assert_eq!(services.try_find_profile_by_name("igual"), Ok(None));
    assert_eq!(services.try_find_profile_by_name("nulo"), Ok(None));
    assert!(matches!(
        services.try_find_profile_by_name("torto"),
        Err(ClientError::Json(_))
    ));
    assert!(matches!(
        services.try_find_profile_by_name("semid"),
        Err(ClientError::Json(_))
    ));
    for name in [
        "vazio", "sumiu", "muitos", "caiu", "igual", "nulo", "torto", "semid",
    ] {
        assert_eq!(services.find_profile_by_name(name), None, "{name}");
        // `findProfileByName` não tenta de novo (as três tentativas do authlib são do `findProfilesByNames`): um pedido por chamada.
        assert_eq!(
            server.count(&format!("{PROFILE_PREFIX}{name}")),
            2,
            "{name}"
        );
    }
}

#[test]
fn a_redirect_is_followed() {
    let server = mojang(|name, _| match name {
        "velho" => Answer::Send(
            302,
            vec![("Location", format!("{PROFILE_PREFIX}notch"))],
            String::new(),
        ),
        _ => ok(NOTCH),
    });
    let services = services(&server, short());
    assert_eq!(
        services.find_profile_by_name("velho"),
        Some((NOTCH_ID, "Notch".to_owned()))
    );
}

#[test]
fn a_service_that_never_answers_gives_up_after_the_five_seconds_of_authlib() {
    let server = mojang(|_, _| Answer::Silent);
    let services = services(&server, Timing::AUTHLIB);
    let started = Instant::now();
    let found = services.try_find_profile_by_name("lento");
    let took = started.elapsed();
    assert!(
        matches!(found, Err(ClientError::ServiceUnavailable(_))),
        "{found:?}"
    );
    assert!(
        took >= Duration::from_millis(4900) && took < Duration::from_millis(7000),
        "{took:?}"
    );
    assert_eq!(server.count(&format!("{PROFILE_PREFIX}lento")), 1);
}

#[test]
fn the_read_timeout_is_for_each_read_and_not_for_the_whole_answer() {
    // Cinco pedaços a 250 ms: 1,25 s no total, com 400 ms de espera por leitura.
    let pieces: Vec<String> = NOTCH
        .as_bytes()
        .chunks(NOTCH.len().div_ceil(5))
        .map(|piece| String::from_utf8(piece.to_vec()).unwrap())
        .collect();
    let server = mojang(move |_, _| Answer::Drip(Duration::from_millis(250), pieces.clone()));
    let services = services(&server, short());
    let started = Instant::now();
    assert_eq!(
        services.try_find_profile_by_name("notch"),
        Ok(Some((NOTCH_ID, "Notch".to_owned())))
    );
    assert!(started.elapsed() >= Duration::from_millis(1000));
    // E um pedaço que demora mais que a espera estoura.
    let server = mojang(|_, _| {
        Answer::Drip(
            Duration::from_millis(900),
            vec!["{".to_owned(), "}".to_owned()],
        )
    });
    let services = self::services(&server, short());
    assert!(matches!(
        services.try_find_profile_by_name("notch"),
        Err(ClientError::ServiceUnavailable(_))
    ));
}

#[test]
fn a_closed_port_is_service_unavailable() {
    // Uma porta que acabou de ser solta: ninguém escuta.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let server = Server::start(move |request, _| match request.path.as_str() {
        "/discovery" => ok(discovery(&format!("http://127.0.0.1:{port}"))),
        _ => Answer::Send(404, Vec::new(), String::new()),
    });
    let services = services(&server, short());
    assert!(matches!(
        services.try_find_profile_by_name("notch"),
        Err(ClientError::ServiceUnavailable(_))
    ));
    assert_eq!(services.find_profile_by_name("notch"), None);
}

#[test]
fn the_lookup_waits_for_the_first_discovery_attempt() {
    let base: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let known = Arc::clone(&base);
    let server = Server::start(move |request, _| match request.path.as_str() {
        "/discovery" => Answer::Late(
            Duration::from_millis(300),
            Box::new(ok(discovery(&known.lock().unwrap()))),
        ),
        _ => ok(NOTCH),
    });
    *base.lock().unwrap() = server.base.clone();
    let services = services(&server, short());
    let started = Instant::now();
    // `ready.join()`: a consulta feita antes de o documento chegar espera por ele.
    assert_eq!(
        services.find_profile_by_name("notch"),
        Some((NOTCH_ID, "Notch".to_owned()))
    );
    assert!(
        started.elapsed() >= Duration::from_millis(280),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn without_the_discovery_document_nobody_is_asked_and_the_fetch_backs_off() {
    // A descoberta falha três vezes e depois responde. O "minuto" do teste tem 100 ms: as tentativas vêm em 0, 100 (1 min) e 300 ms (mais 2 min), e a quarta em
    // 700 ms (mais 4 min).
    let base: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let known = Arc::clone(&base);
    let server = Server::start(move |request, before| match request.path.as_str() {
        "/discovery" if before < 3 => Answer::Send(503, Vec::new(), String::new()),
        "/discovery" => ok(discovery(&known.lock().unwrap())),
        _ => ok(NOTCH),
    });
    *base.lock().unwrap() = server.base.clone();
    let started = Instant::now();
    let services = services(&server, short());
    // Sem documento o serviço está "offline": erro, e nenhum pedido de perfil sai.
    assert_eq!(
        services.try_find_profile_by_name("notch"),
        Err(ClientError::ServiceUnavailable(
            "Services are unavailable".to_owned()
        ))
    );
    assert_eq!(services.find_profile_by_name("notch"), None);
    assert_eq!(server.count(&format!("{PROFILE_PREFIX}notch")), 0);
    let at = |millis: u64| {
        std::thread::sleep(Duration::from_millis(millis).saturating_sub(started.elapsed()))
    };
    at(50);
    assert_eq!(services.attempts(), 1);
    at(200);
    assert_eq!(services.attempts(), 2);
    at(500);
    assert_eq!(services.attempts(), 3);
    assert_eq!(services.find_profile_by_name("notch"), None);
    at(900);
    assert_eq!(services.attempts(), 4);
    // A quarta deu certo: a consulta passa a funcionar, e a próxima busca do documento só vem em 24 "horas".
    assert_eq!(
        services.find_profile_by_name("notch"),
        Some((NOTCH_ID, "Notch".to_owned()))
    );
    at(1400);
    assert_eq!(services.attempts(), 4);
}

#[test]
fn the_backoff_stops_doubling_at_sixty_four_minutes() {
    // O "minuto" com 4 ms: 1, 2, 4, 8, 16, 32, 64, 64, 64... As oito primeiras tentativas cabem em 127 minutos (508 ms), a nona vem em 191 (764 ms) e a décima
    // em 255 (1020 ms); se o intervalo seguisse dobrando, a nona só viria em 255 e a décima em 511 minutos (2044 ms).
    let server = Server::start(|_, _| Answer::Send(503, Vec::new(), String::new()));
    let started = Instant::now();
    let services = services(
        &server,
        Timing {
            minute: Duration::from_millis(4),
            ..short()
        },
    );
    std::thread::sleep(Duration::from_millis(1500).saturating_sub(started.elapsed()));
    let attempts = services.attempts();
    assert!((10..=12).contains(&attempts), "{attempts}");
}

#[test]
fn the_document_is_fetched_again_after_twenty_four_hours() {
    let server = mojang(|_, _| ok(NOTCH));
    // A "hora" com 10 ms: 24 horas são 240 ms.
    let services = services(
        &server,
        Timing {
            hour: Duration::from_millis(10),
            ..short()
        },
    );
    assert!(services.find_profile_by_name("notch").is_some());
    assert_eq!(services.attempts(), 1);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(services.attempts(), 1);
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(services.attempts(), 2);
}

#[test]
fn an_offline_document_or_one_without_the_endpoint_is_service_unavailable() {
    let unavailable = Err(ClientError::ServiceUnavailable(
        "Services are unavailable".to_owned(),
    ));
    let offline = serde_json::json!({"environment": "offline", "product": "minecraft", "discovery": {"profiles": {"endpoints": {"getByName": {"uri": "http://x/{name}"}}}}});
    assert_eq!(
        profile_lookup::endpoint_uri(Some(&offline), "profiles", "getByName"),
        unavailable
    );
    assert_eq!(
        profile_lookup::endpoint_uri(None, "profiles", "getByName"),
        unavailable
    );
    let real: serde_json::Value = serde_json::from_str(&discovery("http://x")).unwrap();
    assert_eq!(
        profile_lookup::endpoint_uri(Some(&real), "profiles", "getByName"),
        Ok(format!("http://x{PROFILE_PREFIX}{{name}}"))
    );
    assert_eq!(
        profile_lookup::endpoint_uri(Some(&real), "profiles", "getById"),
        unavailable
    );
    assert_eq!(
        profile_lookup::endpoint_uri(Some(&real), "session", "getByName"),
        unavailable
    );
    // `getTexture` não tem `uri`.
    assert_eq!(
        profile_lookup::endpoint_uri(Some(&real), "profiles", "getTexture"),
        unavailable
    );
}

#[test]
fn the_id_is_read_as_the_lenient_undashed_uuid_of_authlib() {
    use profile_lookup::undashed_uuid;
    assert_eq!(
        undashed_uuid("069a79f444e94726a5befca90e38aaf5"),
        Some(NOTCH_ID)
    );
    // `fromStringLenient` aceita o UUID que já tem traços.
    assert_eq!(
        undashed_uuid("069a79f4-44e9-4726-a5be-fca90e38aaf5"),
        Some(NOTCH_ID)
    );
    // `UUID.fromString` do Java: grupos curtos valem, com zeros à esquerda.
    assert_eq!(
        undashed_uuid("1-2-3-4-5"),
        Some(0x0000_0001_0002_0003_0004_0000_0000_0005)
    );
    assert_eq!(undashed_uuid("069a79f444e94726a5befca90e38aaf"), None);
    assert_eq!(undashed_uuid("zzzz79f444e94726a5befca90e38aaf5"), None);
    assert_eq!(undashed_uuid(""), None);
    assert_eq!(
        profile_lookup::parse_profile(NOTCH),
        Ok(Some((NOTCH_ID, "Notch".to_owned())))
    );
    assert_eq!(profile_lookup::parse_profile("null"), Ok(None));
}

#[test]
fn a_refresh_that_fails_keeps_the_document_already_read() {
    // A primeira busca dá certo; a segunda (24 "horas" depois) volta vazia e a terceira dá erro: o documento lido fica valendo (`value` só é trocado por outro).
    let base: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let known = Arc::clone(&base);
    let server = Server::start(move |request, before| match request.path.as_str() {
        "/discovery" if before == 0 => ok(discovery(&known.lock().unwrap())),
        "/discovery" if before == 1 => ok(""),
        "/discovery" => Answer::Send(503, Vec::new(), String::new()),
        _ => ok(NOTCH),
    });
    *base.lock().unwrap() = server.base.clone();
    let started = Instant::now();
    // A "hora" com 10 ms: as buscas vêm em 0, 240 e 480 ms.
    let services = services(
        &server,
        Timing {
            hour: Duration::from_millis(10),
            ..short()
        },
    );
    assert!(services.find_profile_by_name("notch").is_some());
    std::thread::sleep(Duration::from_millis(360).saturating_sub(started.elapsed()));
    assert_eq!(services.attempts(), 2);
    assert!(services.find_profile_by_name("notch").is_some());
    std::thread::sleep(Duration::from_millis(600).saturating_sub(started.elapsed()));
    assert_eq!(services.attempts(), 3);
    assert!(services.find_profile_by_name("notch").is_some());
    // Com um documento na mão a próxima busca segue em 24 "horas", não no minuto da falha.
    std::thread::sleep(Duration::from_millis(840).saturating_sub(started.elapsed()));
    assert_eq!(services.attempts(), 4);
}
