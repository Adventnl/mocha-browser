//! Subresource discovery and loading for Mocha Browser.
//!
//! Milestone 8 loads the stylesheets a document references: inline `<style>`
//! blocks and external `<link rel="stylesheet">` resources. Stylesheets are
//! returned in document order so the cascade's "later wins" tie-break stays
//! correct. External resources are resolved against the document's base URL and
//! loaded through `mocha_net`; their content type is validated and a failed load
//! is a clear error (never silently ignored).
//!
//! Out of scope (documented): external `<script src>`, CSS `url(...)` resources,
//! web fonts, and a `<base>` element. Image discovery for replaced elements is
//! added alongside Milestone 9.

use mocha_css::{parse_stylesheet, Stylesheet};
use mocha_dom::{Document, ElementData, NodeId, NodeKind};
use mocha_error::{MochaError, MochaResult};
use mocha_image::{DecodedImage, RasterImage};
use mocha_net::{LoadRequest, ResourceLoader, ResourceResponse, ResourceType};
use mocha_url::Url;

/// Collect every document stylesheet in document order, loading external
/// `<link rel="stylesheet">` resources against `base` through `loader`.
///
/// `<link>` elements whose `rel` is not `stylesheet` are ignored (as browsers do
/// for unknown link relations). A `<link rel="stylesheet">` without an `href`, a
/// non-`text/css` response, or a failed load is a clear error.
pub fn collect_document_stylesheets(
    document: &Document,
    base: &Url,
    loader: &mut dyn ResourceLoader,
) -> MochaResult<Vec<Stylesheet>> {
    let mut sheets = Vec::new();
    for id in document.traverse_depth_first(document.root_id())? {
        let NodeKind::Element(data) = &document.node(id)?.kind else {
            continue;
        };
        match data.tag_name.as_str() {
            "style" => sheets.push(parse_style_element(document, id)?),
            "link" => {
                if !is_stylesheet_link(data) {
                    continue;
                }
                let href = stylesheet_href(data)?;
                sheets.push(load_external_stylesheet(&href, base, loader)?);
            }
            _ => {}
        }
    }
    Ok(sheets)
}

/// Collect only inline `<style>` blocks, for in-memory rendering with no base URL.
///
/// A `<link rel="stylesheet">` here is reported as unsupported, since it cannot be
/// resolved without a document base URL.
pub fn collect_inline_stylesheets(document: &Document) -> MochaResult<Vec<Stylesheet>> {
    let mut sheets = Vec::new();
    for id in document.traverse_depth_first(document.root_id())? {
        let NodeKind::Element(data) = &document.node(id)?.kind else {
            continue;
        };
        match data.tag_name.as_str() {
            "style" => sheets.push(parse_style_element(document, id)?),
            "link" if is_stylesheet_link(data) => {
                return Err(MochaError::UnsupportedFeature(
                    "external stylesheets need a document base URL (load via file/http, not in-memory HTML)"
                        .to_string(),
                ));
            }
            _ => {}
        }
    }
    Ok(sheets)
}

/// Discover every `<img>` element with a `src`, returning `(node, src)` pairs in
/// document order. An `<img>` without a (non-empty) `src` is a clear error.
pub fn discover_images(document: &Document) -> MochaResult<Vec<(NodeId, String)>> {
    let mut images = Vec::new();
    for id in document.traverse_depth_first(document.root_id())? {
        let NodeKind::Element(data) = &document.node(id)?.kind else {
            continue;
        };
        if data.tag_name != "img" {
            continue;
        }
        let src = data
            .attribute("src")
            .filter(|src| !src.is_empty())
            .ok_or_else(|| MochaError::Layout("<img> is missing its src attribute".to_string()))?;
        images.push((id, src.to_string()));
    }
    Ok(images)
}

/// Resolve `src` against `base`, load it through `loader`, validate that it is an
/// image, and decode its intrinsic dimensions.
pub fn load_image(
    src: &str,
    base: &Url,
    loader: &mut dyn ResourceLoader,
) -> MochaResult<DecodedImage> {
    mocha_image::decode(&fetch_image_bytes(src, base, loader)?)
}

/// Like [`load_image`], but decodes the full RGBA pixels for the software
/// rasterizer (Milestone 11).
pub fn load_image_rgba(
    src: &str,
    base: &Url,
    loader: &mut dyn ResourceLoader,
) -> MochaResult<RasterImage> {
    mocha_image::decode_rgba(&fetch_image_bytes(src, base, loader)?)
}

/// Fetch and validate an image resource, returning its raw bytes.
fn fetch_image_bytes(
    src: &str,
    base: &Url,
    loader: &mut dyn ResourceLoader,
) -> MochaResult<Vec<u8>> {
    let url = base.join(src)?;
    let response = loader.load(LoadRequest::get(url))?;
    if let Some(status) = response.status {
        if !(200..300).contains(&status) {
            return Err(MochaError::Network(format!(
                "image request failed with status {status}"
            )));
        }
    }
    validate_image_content_type(&response)?;
    Ok(response.body)
}

/// Reject responses whose content type is clearly not an image. A missing content
/// type or `application/octet-stream` is allowed — the decoder validates the bytes.
fn validate_image_content_type(response: &ResourceResponse) -> MochaResult<()> {
    if let Some(content_type) = &response.content_type {
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if mime.is_empty() || mime.starts_with("image/") || mime == "application/octet-stream" {
            return Ok(());
        }
        return Err(MochaError::Network(format!(
            "image response has a non-image content type: {mime}"
        )));
    }
    Ok(())
}

fn is_stylesheet_link(data: &ElementData) -> bool {
    data.attribute("rel")
        .map(|rel| rel.eq_ignore_ascii_case("stylesheet"))
        .unwrap_or(false)
}

fn stylesheet_href(data: &ElementData) -> MochaResult<String> {
    data.attribute("href").map(str::to_string).ok_or_else(|| {
        MochaError::Parse(r#"<link rel="stylesheet"> is missing its href"#.to_string())
    })
}

fn parse_style_element(document: &Document, id: NodeId) -> MochaResult<Stylesheet> {
    let mut css = String::new();
    for &child in document.children(id)? {
        if let NodeKind::Text(text) = &document.node(child)?.kind {
            css.push_str(&text.text);
            css.push(' ');
        }
    }
    parse_stylesheet(&css)
}

fn load_external_stylesheet(
    href: &str,
    base: &Url,
    loader: &mut dyn ResourceLoader,
) -> MochaResult<Stylesheet> {
    let url = base.join(href)?;
    let response = loader.load(LoadRequest::get(url))?;
    if let Some(status) = response.status {
        if !(200..300).contains(&status) {
            return Err(MochaError::Network(format!(
                "stylesheet request failed with status {status}"
            )));
        }
    }
    if response.resource_type() != ResourceType::Css {
        return Err(MochaError::Network(format!(
            "stylesheet response is not text/css (content-type: {})",
            response.content_type.as_deref().unwrap_or("none")
        )));
    }
    let css = std::str::from_utf8(&response.body)
        .map_err(|_| MochaError::Network("stylesheet is not valid UTF-8".to_string()))?;
    parse_stylesheet(css)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mocha_css::{Color, CssValue};
    use mocha_html::parse_html;
    use mocha_net::test_server::{Reply, TestServer};
    use mocha_net::DefaultLoader;

    /// The `color` value of the first rule in `sheet` (the test sheets each carry
    /// exactly one `color` rule), or `None`.
    fn sheet_color(sheet: &Stylesheet) -> Option<CssValue> {
        sheet
            .rules
            .iter()
            .flat_map(|rule| &rule.declarations)
            .find(|decl| decl.property == mocha_css::CssProperty::Color)
            .map(|decl| decl.value.clone())
    }

    #[test]
    fn inline_style_only_is_collected() {
        let document =
            parse_html("<html><body><style>p { color: red; }</style></body></html>").unwrap();
        let sheets = collect_inline_stylesheets(&document).unwrap();
        assert_eq!(sheets.len(), 1);
        assert_eq!(
            sheet_color(&sheets[0]),
            Some(CssValue::Color(Color::rgb(255, 0, 0)))
        );
    }

    #[test]
    fn inline_collection_rejects_external_link() {
        let document =
            parse_html(r#"<html><body><link rel="stylesheet" href="a.css"></body></html>"#)
                .unwrap();
        assert!(matches!(
            collect_inline_stylesheets(&document).unwrap_err(),
            MochaError::UnsupportedFeature(_)
        ));
    }

    #[test]
    fn external_stylesheet_over_http_is_loaded_in_document_order() {
        let server = TestServer::start(vec![(
            "/site.css".to_string(),
            Reply::Css("p { color: blue; }".to_string()),
        )]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let document = parse_html(
            r#"<html><body><style>p { color: red; }</style><link rel="stylesheet" href="site.css"></body></html>"#,
        )
        .unwrap();
        let mut loader = DefaultLoader::new();
        let sheets = collect_document_stylesheets(&document, &base, &mut loader).unwrap();
        // Two sheets in document order: the inline red <style>, then the external
        // blue <link>.
        assert_eq!(sheets.len(), 2);
        assert_eq!(
            sheet_color(&sheets[0]),
            Some(CssValue::Color(Color::rgb(255, 0, 0)))
        );
        assert_eq!(
            sheet_color(&sheets[1]),
            Some(CssValue::Color(Color::rgb(0, 0, 255)))
        );
    }

    #[test]
    fn missing_stylesheet_errors_clearly() {
        let server = TestServer::start(vec![]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let document =
            parse_html(r#"<html><body><link rel="stylesheet" href="missing.css"></body></html>"#)
                .unwrap();
        let mut loader = DefaultLoader::new();
        let error = collect_document_stylesheets(&document, &base, &mut loader).unwrap_err();
        assert!(matches!(error, MochaError::Network(_)));
    }

    #[test]
    fn wrong_content_type_is_rejected() {
        let server = TestServer::start(vec![(
            "/styles".to_string(),
            Reply::Html("<html></html>".to_string()),
        )]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let document =
            parse_html(r#"<html><body><link rel="stylesheet" href="styles"></body></html>"#)
                .unwrap();
        let mut loader = DefaultLoader::new();
        let error = collect_document_stylesheets(&document, &base, &mut loader).unwrap_err();
        assert!(matches!(error, MochaError::Network(_)));
    }

    #[test]
    fn link_without_href_errors() {
        let document = parse_html(r#"<html><body><link rel="stylesheet"></body></html>"#).unwrap();
        let server = TestServer::start(vec![]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let mut loader = DefaultLoader::new();
        assert!(matches!(
            collect_document_stylesheets(&document, &base, &mut loader).unwrap_err(),
            MochaError::Parse(_)
        ));
    }

    #[test]
    fn non_stylesheet_link_is_ignored() {
        let document =
            parse_html(r#"<html><body><link rel="icon" href="favicon.ico"><style>p { color: red; }</style></body></html>"#)
                .unwrap();
        let server = TestServer::start(vec![]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let mut loader = DefaultLoader::new();
        let sheets = collect_document_stylesheets(&document, &base, &mut loader).unwrap();
        // Only the <style> is collected; the non-stylesheet link is ignored.
        assert_eq!(sheets.len(), 1);
    }

    // --- images (Milestone 9) -----------------------------------------------

    fn sample_png(width: u32, height: u32) -> Vec<u8> {
        let mut buffer = Vec::new();
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut buffer),
                image::ImageFormat::Png,
            )
            .unwrap();
        buffer
    }

    #[test]
    fn discover_images_finds_src_in_document_order() {
        let document =
            parse_html(r#"<html><body><img src="a.png"><p>x</p><img src="b.png"></body></html>"#)
                .unwrap();
        let images = discover_images(&document).unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].1, "a.png");
        assert_eq!(images[1].1, "b.png");
    }

    #[test]
    fn discover_images_errors_on_missing_src() {
        let document = parse_html(r#"<html><body><img alt="no source"></body></html>"#).unwrap();
        assert!(matches!(
            discover_images(&document).unwrap_err(),
            MochaError::Layout(_)
        ));
    }

    #[test]
    fn load_image_over_http_decodes_dimensions() {
        let server = TestServer::start(vec![(
            "/pic.png".to_string(),
            Reply::Bytes {
                content_type: "image/png".to_string(),
                body: sample_png(5, 7),
            },
        )]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let mut loader = DefaultLoader::new();
        let decoded = load_image("pic.png", &base, &mut loader).unwrap();
        assert_eq!((decoded.width, decoded.height), (5, 7));
    }

    #[test]
    fn load_image_rgba_over_http_decodes_pixels() {
        let server = TestServer::start(vec![(
            "/pic.png".to_string(),
            Reply::Bytes {
                content_type: "image/png".to_string(),
                body: sample_png(3, 2),
            },
        )]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let mut loader = DefaultLoader::new();
        let raster = load_image_rgba("pic.png", &base, &mut loader).unwrap();
        assert_eq!((raster.width, raster.height), (3, 2));
        assert_eq!(raster.rgba.len(), 3 * 2 * 4);
    }

    #[test]
    fn load_image_rejects_non_image_content_type() {
        let server = TestServer::start(vec![(
            "/pic.png".to_string(),
            Reply::Text("not an image".to_string()),
        )]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let mut loader = DefaultLoader::new();
        assert!(matches!(
            load_image("pic.png", &base, &mut loader).unwrap_err(),
            MochaError::Network(_)
        ));
    }

    #[test]
    fn load_image_missing_resource_errors() {
        let server = TestServer::start(vec![]);
        let base = Url::parse(&server.url("/index.html")).unwrap();
        let mut loader = DefaultLoader::new();
        assert!(load_image("gone.png", &base, &mut loader).is_err());
    }
}
