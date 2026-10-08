//! The request bodies, byte for byte.

use super::{CompletedPart, complete_multipart_upload_body, delete_objects_body};
use crate::xml::parse_tree;

#[test]
fn complete_multipart_lists_parts_with_explicit_numbers() {
    let parts = [
        CompletedPart {
            number: 1,
            etag: "\"a54357aff0632cce46d942af68356b38\"".into(),
        },
        CompletedPart {
            number: 2,
            etag: "\"0c78aef83f66abc1fa1e8477f296d394\"".into(),
        },
    ];

    assert_eq!(
        complete_multipart_upload_body(&parts),
        "<CompleteMultipartUpload xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
         <Part><PartNumber>1</PartNumber><ETag>&quot;a54357aff0632cce46d942af68356b38&quot;</ETag></Part>\
         <Part><PartNumber>2</PartNumber><ETag>&quot;0c78aef83f66abc1fa1e8477f296d394&quot;</ETag></Part>\
         </CompleteMultipartUpload>"
    );
}

#[test]
fn delete_lists_objects_and_says_quiet() {
    assert_eq!(
        delete_objects_body(&["a.txt", "dir/b.txt"], true),
        "<Delete xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Quiet>true</Quiet>\
         <Object><Key>a.txt</Key></Object><Object><Key>dir/b.txt</Key></Object></Delete>"
    );
    assert!(delete_objects_body(&["a"], false).contains("<Quiet>false</Quiet>"));
}

#[test]
fn keys_are_escaped_including_the_line_breaks_xml_would_normalize_away() {
    // XML parsers turn a literal CR or CRLF into LF, so a key holding one would
    // name a different object; AWS's key-naming guide says to send them as
    // character references.
    let body = delete_objects_body(&["a&b<c>\"d'\r\n e "], true);

    assert!(body.contains("<Key>a&amp;b&lt;c&gt;&quot;d&apos;&#13;&#10; e </Key>"));
    // And it reads back as the same key.
    let tree = parse_tree(&body).unwrap();
    assert_eq!(tree.child("Object").unwrap().raw("Key"), Some("a&b<c>\"d'\r\n e "));
}
