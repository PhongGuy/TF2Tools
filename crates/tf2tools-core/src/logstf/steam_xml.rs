//! A tiny text extractor for Steam profile XML bodies.

/// The text of the first `<tag>` element, CDATA unwrapped and the five XML entities decoded.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "resolve_player in Task 3 calls it")
)]
pub(crate) fn xml_text(body: &str, tag: &str) -> Option<String> {
    // The `>` in the needle keeps `<steamID>` from matching `<steamID64>`.
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = body.find(&open)? + open.len();
    let len = body.get(start..)?.find(&close)?;
    let inner = body.get(start..start + len)?.trim();
    // CDATA is literal text; only bare text carries entities.
    if let Some(cdata) = inner
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
    {
        return Some(cdata.to_owned());
    }
    // `&amp;` goes last so `&amp;lt;` decodes to `&lt;`, not `<`.
    Some(
        inner
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

#[cfg(test)]
mod tests {
    use super::xml_text;

    // Shapes recorded in docs/research/2026-11-10-logstf-api.md.
    const FOUND: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <profile><steamID64>76561197960287930</steamID64><steamID><![CDATA[Rabscuttle]]></steamID>\
        <onlineState>offline</onlineState><privacyState>friendsonly</privacyState>\
        <avatarMedium><![CDATA[https://avatars.akamai.steamstatic.com/abc_medium.jpg]]></avatarMedium></profile>";
    const NOT_FOUND: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <response><error><![CDATA[The specified profile could not be found.]]></error></response>";

    #[test]
    fn reads_plain_text_and_cdata() {
        assert_eq!(
            xml_text(FOUND, "steamID64").as_deref(),
            Some("76561197960287930")
        );
        assert_eq!(xml_text(FOUND, "steamID").as_deref(), Some("Rabscuttle"));
        assert_eq!(
            xml_text(FOUND, "avatarMedium").as_deref(),
            Some("https://avatars.akamai.steamstatic.com/abc_medium.jpg")
        );
        assert_eq!(
            xml_text(NOT_FOUND, "error").as_deref(),
            Some("The specified profile could not be found.")
        );
    }

    #[test]
    fn a_missing_or_unclosed_tag_is_none() {
        assert_eq!(xml_text(FOUND, "error"), None);
        assert_eq!(xml_text(NOT_FOUND, "steamID64"), None);
        assert_eq!(xml_text("<steamID>open", "steamID"), None);
    }

    #[test]
    fn entities_decode_outside_cdata_only() {
        assert_eq!(
            xml_text(
                "<steamID>A &amp; B &lt;3&gt; &quot;x&quot; &apos;y&apos;</steamID>",
                "steamID"
            )
            .as_deref(),
            Some("A & B <3> \"x\" 'y'")
        );
        assert_eq!(
            xml_text("<steamID><![CDATA[A &amp; B]]></steamID>", "steamID").as_deref(),
            Some("A &amp; B")
        );
    }
}
