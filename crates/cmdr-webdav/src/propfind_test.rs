//! Two servers' dialects, one parser.

use super::{PropfindEntry, decode_href, parse_multistatus, parse_rfc3339};

const NEXTCLOUD: &str = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:oc="http://owncloud.org/ns">
  <d:response>
    <d:href>/remote.php/dav/files/ada/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/></d:resourcetype>
        <d:getlastmodified>Fri, 01 Mar 2024 12:34:56 GMT</d:getlastmodified>
        <d:quota-available-bytes>-3</d:quota-available-bytes>
        <d:quota-used-bytes>1024</d:quota-used-bytes>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
    <d:propstat>
      <d:prop><d:getcontentlength/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/remote.php/dav/files/ada/Photos%20from%20Zs%C3%B3fi/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/></d:resourcetype>
        <d:getlastmodified>Fri, 01 Mar 2024 12:34:56 GMT</d:getlastmodified>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/remote.php/dav/files/ada/notes.txt</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype/>
        <d:getcontentlength>42</d:getcontentlength>
        <d:getlastmodified>Fri, 01 Mar 2024 12:34:56 GMT</d:getlastmodified>
        <d:creationdate>2024-03-01T12:00:00Z</d:creationdate>
        <d:getetag>"abc"</d:getetag>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

const APACHE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:ns0="DAV:">
<D:response xmlns:lp1="DAV:" xmlns:lp2="http://apache.org/dav/props/">
<D:href>/dav/</D:href>
<D:propstat>
<D:prop>
<lp1:resourcetype><D:collection/></lp1:resourcetype>
<lp1:creationdate>2024-03-01T12:00:00Z</lp1:creationdate>
<lp1:getlastmodified>Fri, 01 Mar 2024 12:34:56 GMT</lp1:getlastmodified>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>
<D:response xmlns:lp1="DAV:">
<D:href>/dav/large.bin</D:href>
<D:propstat>
<D:prop>
<lp1:resourcetype/>
<lp1:getcontentlength>4194304</lp1:getcontentlength>
<lp1:getlastmodified>Fri, 01 Mar 2024 12:34:56 GMT</lp1:getlastmodified>
</D:prop>
<D:status>HTTP/1.1 200 OK</D:status>
</D:propstat>
</D:response>
</D:multistatus>"#;

/// 2024-03-01T12:34:56Z.
const STAMP: u64 = 1_709_296_496;

#[test]
fn a_nextcloud_listing_yields_decoded_hrefs_and_typed_props() {
    let entries = parse_multistatus(NEXTCLOUD).expect("a multistatus");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].href, "/remote.php/dav/files/ada/");
    assert!(entries[0].is_collection);
    assert_eq!(entries[0].size, None, "a 404 propstat never contributes a size");
    assert_eq!(entries[0].quota_available, Some(-3));
    assert_eq!(entries[0].quota_used, Some(1024));
    assert_eq!(entries[1].href, "/remote.php/dav/files/ada/Photos from Zsófi/");
    assert!(entries[1].is_collection);
    assert_eq!(
        entries[2],
        PropfindEntry {
            href: "/remote.php/dav/files/ada/notes.txt".into(),
            is_collection: false,
            size: Some(42),
            modified_at: Some(STAMP),
            created_at: Some(STAMP - 34 * 60 - 56),
            quota_available: None,
            quota_used: None,
        }
    );
}

#[test]
fn an_apache_listing_parses_with_its_own_prefixes() {
    let entries = parse_multistatus(APACHE).expect("a multistatus");
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_collection);
    assert_eq!(entries[0].created_at, Some(STAMP - 34 * 60 - 56));
    assert!(!entries[1].is_collection);
    assert_eq!(entries[1].size, Some(4_194_304));
    assert_eq!(entries[1].modified_at, Some(STAMP));
}

#[test]
fn an_html_page_is_not_a_multistatus() {
    assert!(parse_multistatus("<html><body>Welcome</body></html>").is_err());
    assert!(parse_multistatus("not xml at all <<<").is_err());
}

#[test]
fn an_absolute_href_is_reduced_to_its_path() {
    assert_eq!(decode_href("http://127.0.0.1:13480/dav/a%20b/"), "/dav/a b/");
    assert_eq!(decode_href("/dav/%C3%A9.txt"), "/dav/é.txt");
}

#[test]
fn creation_dates_read_with_and_without_an_offset() {
    assert_eq!(parse_rfc3339("2024-03-01T12:34:56Z"), Some(STAMP));
    assert_eq!(parse_rfc3339("2024-03-01T13:34:56+01:00"), Some(STAMP));
    assert_eq!(parse_rfc3339("2024-03-01T12:34:56.123Z"), Some(STAMP));
    assert_eq!(parse_rfc3339("yesterday"), None);
}
