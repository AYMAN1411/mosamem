//! `lawha://` serves tiles to the webview:
//! `lawha://localhost/composite/<doc>/<version>/<lod>/<tx>/<ty>` → 256×256 RGBA8, or 204 if empty.
//! `lawha://localhost/live/<session>/<below|layer|above>/<lod>/<tx>/<ty>` → live-session parts.
//! `lawha://localhost/thumb/<doc>/<layer>/<key>/<size>` → layer thumbnail (see doc::thumbs).

use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::doc::{composite, live, thumbs, Engine};

fn reply(status: StatusCode, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/octet-stream")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cross-Origin-Resource-Policy", "cross-origin")
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap()
}

pub fn handle<R: Runtime>(ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().to_owned();
    rayon::spawn(move || {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let response = match parts.as_slice() {
            ["composite", doc, _version, lod, tx, ty] => {
                let parsed = (doc.parse::<u64>(), lod.parse::<u8>(), tx.parse::<i32>(), ty.parse::<i32>());
                match parsed {
                    (Ok(doc), Ok(lod), Ok(tx), Ok(ty)) if lod <= composite::MAX_LOD => {
                        match app.state::<Engine>().get(doc) {
                            Some(h) => match composite::tile(&h, lod, tx, ty) {
                                Some(t) => reply(StatusCode::OK, t.0.to_vec()),
                                None => reply(StatusCode::NO_CONTENT, Vec::new()),
                            },
                            None => reply(StatusCode::NOT_FOUND, Vec::new()),
                        }
                    }
                    _ => reply(StatusCode::BAD_REQUEST, Vec::new()),
                }
            }
            ["live", session, part, lod, tx, ty] => {
                let part = match *part {
                    "below" => Some(live::Part::Below),
                    "layer" => Some(live::Part::Layer),
                    "above" => Some(live::Part::Above),
                    _ => None,
                };
                let s = session.parse::<u64>().ok().and_then(|id| app.state::<Engine>().live(id));
                match (s, part, lod.parse::<u8>(), tx.parse::<i32>(), ty.parse::<i32>()) {
                    (Some(s), Some(part), Ok(lod), Ok(tx), Ok(ty)) if lod <= composite::MAX_LOD => match s.tile(part, lod, tx, ty) {
                        Some(t) => reply(StatusCode::OK, t.0.to_vec()),
                        None => reply(StatusCode::NO_CONTENT, Vec::new()),
                    },
                    (None, ..) => reply(StatusCode::GONE, Vec::new()),
                    _ => reply(StatusCode::BAD_REQUEST, Vec::new()),
                }
            }
            ["thumb", doc, layer, _key, size] => {
                let h = doc.parse::<u64>().ok().and_then(|d| app.state::<Engine>().get(d));
                match (h, layer.parse::<u64>(), size.parse::<u32>()) {
                    (Some(h), Ok(layer), Ok(size)) if (8..=512).contains(&size) => {
                        let found = {
                            let d = h.doc.read();
                            d.content.find(layer).and_then(|n| n.tiles().cloned()).map(|t| (t, d.content.width, d.content.height))
                        };
                        match found {
                            Some((tiles, w, ht)) => reply(StatusCode::OK, thumbs::thumbnail(&h.thumbs, &tiles, w, ht, size).to_vec()),
                            None => reply(StatusCode::NO_CONTENT, Vec::new()),
                        }
                    }
                    _ => reply(StatusCode::BAD_REQUEST, Vec::new()),
                }
            }
            _ => reply(StatusCode::NOT_FOUND, Vec::new()),
        };
        responder.respond(response);
    });
}
