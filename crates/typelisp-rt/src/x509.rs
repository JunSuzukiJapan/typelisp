//! The subject of an X.509 certificate, as a string — what `peer-subject`
//! answers for a TLS peer that presented one.
//!
//! `rustls` verifies certificates but exposes nothing of their contents
//! beyond the DER bytes, and the crates that parse X.509 in full bring a
//! dozen dependencies for the one field a program asks about: *who is
//! this*. So the walk to that field is written here: through the outer
//! `SEQUENCE`, into `TBSCertificate`, past the version, serial, signature
//! algorithm, issuer and validity, to `subject` — a `Name`, which is a
//! `SEQUENCE OF` relative distinguished names, each a `SET OF` attribute
//! type and value.
//!
//! The string is RFC 4514's form, the one `openssl`'s `-nameopt RFC2253`
//! and Go's `pkix.Name.String()` print: `CN=client,O=Example,C=JP` — the
//! RDNs in reverse of their DER order (the most specific first), attribute
//! names for the types every certificate uses and the dotted OID for any
//! other, values escaped as the RFC says, and a value whose encoding is not
//! a string written as `#` and its hex — the RFC's own spelling for that
//! case, so that no certificate a verifier accepted is one this cannot
//! render.

/// The subject of the DER-encoded certificate `der`, in RFC 4514 form. An
/// error names what in the encoding was not X.509 — a certificate that has
/// been through the verifier never gets one.
pub fn subject(der: &[u8]) -> Result<String, String> {
    let (certificate, _) = Tlv::read(der, 0).map_err(|e| format!("certificate: {}", e))?;
    certificate.expect(0x30, "certificate")?;
    let (tbs, _) = Tlv::read(certificate.content, 0).map_err(|e| format!("tbsCertificate: {}", e))?;
    tbs.expect(0x30, "tbsCertificate")?;
    let body = tbs.content;
    let mut pos = 0;
    // version [0] EXPLICIT, optional — v1 certificates omit it.
    let (first, after_first) = Tlv::read(body, pos).map_err(|e| format!("tbsCertificate: {}", e))?;
    if first.tag == 0xA0 {
        pos = after_first;
    }
    // serialNumber, signature, issuer, validity: skipped in order.
    for (field, tag) in [("serialNumber", 0x02), ("signature", 0x30), ("issuer", 0x30), ("validity", 0x30)] {
        let (tlv, next) = Tlv::read(body, pos).map_err(|e| format!("{}: {}", field, e))?;
        tlv.expect(tag, field)?;
        pos = next;
    }
    let (name, _) = Tlv::read(body, pos).map_err(|e| format!("subject: {}", e))?;
    name.expect(0x30, "subject")?;
    render_name(name.content)
}

/// One DER tag-length-value, with `content` borrowed from the input.
struct Tlv<'a> {
    tag: u8,
    content: &'a [u8],
}

impl<'a> Tlv<'a> {
    /// The TLV at `pos`, and the position after it. Definite lengths only,
    /// which is all DER allows.
    fn read(buf: &'a [u8], pos: usize) -> Result<(Tlv<'a>, usize), String> {
        let tag = *buf.get(pos).ok_or("truncated before a tag")?;
        if tag & 0x1F == 0x1F {
            return Err(format!("multi-byte tag at {}", pos));
        }
        let first = *buf.get(pos + 1).ok_or("truncated before a length")?;
        let (len, header) = if first < 0x80 {
            (first as usize, 2)
        } else {
            let n = (first & 0x7F) as usize;
            if n == 0 || n > 4 {
                return Err(format!("length of {} bytes at {}", n, pos));
            }
            let bytes = buf.get(pos + 2..pos + 2 + n).ok_or("truncated inside a length")?;
            (bytes.iter().fold(0usize, |acc, b| (acc << 8) | *b as usize), 2 + n)
        };
        let start = pos + header;
        let content = buf.get(start..start + len).ok_or("truncated inside a value")?;
        Ok((Tlv { tag, content }, start + len))
    }

    fn expect(&self, tag: u8, what: &str) -> Result<(), String> {
        if self.tag == tag {
            Ok(())
        } else {
            Err(format!("{}: expected tag {:#04x}, found {:#04x}", what, tag, self.tag))
        }
    }
}

/// A `Name`'s content: RDNs, each rendered and joined by `,` in reverse
/// order.
fn render_name(content: &[u8]) -> Result<String, String> {
    let mut rdns = Vec::new();
    let mut pos = 0;
    while pos < content.len() {
        let (set, next) = Tlv::read(content, pos).map_err(|e| format!("subject: {}", e))?;
        set.expect(0x31, "relativeDistinguishedName")?;
        rdns.push(render_rdn(set.content)?);
        pos = next;
    }
    rdns.reverse();
    Ok(rdns.join(","))
}

/// A `SET OF AttributeTypeAndValue`: usually one, joined by `+` when more.
fn render_rdn(content: &[u8]) -> Result<String, String> {
    let mut parts = Vec::new();
    let mut pos = 0;
    while pos < content.len() {
        let (atv, next) = Tlv::read(content, pos).map_err(|e| format!("attribute: {}", e))?;
        atv.expect(0x30, "attributeTypeAndValue")?;
        let (oid, after_oid) = Tlv::read(atv.content, 0).map_err(|e| format!("attribute type: {}", e))?;
        oid.expect(0x06, "attribute type")?;
        let (value, _) = Tlv::read(atv.content, after_oid).map_err(|e| format!("attribute value: {}", e))?;
        parts.push(format!("{}={}", attribute_name(oid.content), render_value(&value)));
        pos = next;
    }
    Ok(parts.join("+"))
}

/// The short name RFC 4514 gives the attribute types certificates use, or
/// the dotted OID.
fn attribute_name(oid: &[u8]) -> String {
    let dotted = dotted_oid(oid);
    let known = match dotted.as_str() {
        "2.5.4.3" => "CN",
        "2.5.4.4" => "SN",
        "2.5.4.5" => "serialNumber",
        "2.5.4.6" => "C",
        "2.5.4.7" => "L",
        "2.5.4.8" => "ST",
        "2.5.4.9" => "STREET",
        "2.5.4.10" => "O",
        "2.5.4.11" => "OU",
        "2.5.4.12" => "title",
        "2.5.4.42" => "GN",
        "2.5.4.43" => "initials",
        "2.5.4.46" => "dnQualifier",
        "2.5.4.65" => "pseudonym",
        "0.9.2342.19200300.100.1.1" => "UID",
        "0.9.2342.19200300.100.1.25" => "DC",
        "1.2.840.113549.1.9.1" => "emailAddress",
        _ => return dotted,
    };
    known.to_string()
}

/// An OID's content bytes as dotted decimal: the first byte holds the first
/// two arcs (`40 * a + b`), the rest are base-128 with a continuation bit.
fn dotted_oid(bytes: &[u8]) -> String {
    let mut arcs: Vec<u64> = Vec::new();
    let mut acc: u64 = 0;
    for b in bytes {
        acc = (acc << 7) | (*b & 0x7F) as u64;
        if b & 0x80 == 0 {
            if arcs.is_empty() {
                let first = if acc < 40 {
                    0
                } else if acc < 80 {
                    1
                } else {
                    2
                };
                arcs.push(first);
                arcs.push(acc - 40 * first);
            } else {
                arcs.push(acc);
            }
            acc = 0;
        }
    }
    arcs.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(".")
}

/// A value as text: every string type a `DirectoryString` or an IA5
/// address can be, escaped as RFC 4514 §2.4 requires; anything else as `#`
/// and the hex of its whole encoding (§2.4's last rule).
fn render_value(value: &Tlv<'_>) -> String {
    let text = match value.tag {
        // UTF8String, PrintableString, IA5String: bytes are the text.
        0x0C | 0x13 | 0x16 => String::from_utf8(value.content.to_vec()).ok(),
        // TeletexString: read as Latin-1, which is what every tool does with
        // the characters that occur in practice.
        0x14 => Some(value.content.iter().map(|b| *b as char).collect()),
        // BMPString: UTF-16, big-endian.
        0x1E if value.content.len().is_multiple_of(2) => {
            let units: Vec<u16> = value.content.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            String::from_utf16(&units).ok()
        }
        // UniversalString: UTF-32, big-endian.
        0x1C if value.content.len().is_multiple_of(4) => value
            .content
            .chunks(4)
            .map(|c| char::from_u32(u32::from_be_bytes([c[0], c[1], c[2], c[3]])))
            .collect::<Option<String>>(),
        _ => None,
    };
    match text {
        Some(text) => escape(&text),
        None => {
            let mut hex = String::from("#");
            hex.push_str(&format!("{:02x}", value.tag));
            // Length re-encoded the short or long way, as DER wrote it.
            let len = value.content.len();
            if len < 0x80 {
                hex.push_str(&format!("{:02x}", len));
            } else {
                let bytes = len.to_be_bytes();
                let significant: Vec<u8> = bytes.iter().copied().skip_while(|b| *b == 0).collect();
                hex.push_str(&format!("{:02x}", 0x80 | significant.len()));
                for b in significant {
                    hex.push_str(&format!("{:02x}", b));
                }
            }
            for b in value.content {
                hex.push_str(&format!("{:02x}", b));
            }
            hex
        }
    }
}

/// RFC 4514 §2.4: a leading `#` or space, a trailing space, and the
/// characters `" + , ; < > \` anywhere are escaped with a backslash; NUL
/// as `\00`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let special = matches!(c, '"' | '+' | ',' | ';' | '<' | '>' | '\\')
            || (i == 0 && (*c == '#' || *c == ' '))
            || (i == chars.len() - 1 && *c == ' ');
        if *c == '\0' {
            out.push_str("\\00");
        } else if special {
            out.push('\\');
            out.push(*c);
        } else {
            out.push(*c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cert_with(dn: &[(rcgen::DnType, &str)]) -> Vec<u8> {
        let mut params = rcgen::CertificateParams::new(vec!["example.test".to_string()]).unwrap();
        let mut name = rcgen::DistinguishedName::new();
        for (t, v) in dn {
            name.push(t.clone(), *v);
        }
        params.distinguished_name = name;
        let key = rcgen::KeyPair::generate().unwrap();
        params.self_signed(&key).unwrap().der().to_vec()
    }

    #[test]
    fn the_subject_reads_most_specific_first() {
        use rcgen::DnType::*;
        let der = cert_with(&[(CountryName, "JP"), (OrganizationName, "Example"), (CommonName, "client")]);
        assert_eq!(subject(&der).unwrap(), "CN=client,O=Example,C=JP");
    }

    #[test]
    fn special_characters_are_escaped_and_unknown_types_are_dotted() {
        use rcgen::DnType::*;
        let der = cert_with(&[(CommonName, "a, b+c"), (CustomDnType(vec![2, 5, 4, 97]), "VATJP-1")]);
        assert_eq!(subject(&der).unwrap(), "2.5.4.97=VATJP-1,CN=a\\, b\\+c");
    }

    #[test]
    fn a_non_string_value_is_written_as_hex() {
        let tlv = Tlv { tag: 0x02, content: &[0x01, 0x02] };
        assert_eq!(render_value(&tlv), "#02020102");
    }

    #[test]
    fn oids_decode() {
        assert_eq!(dotted_oid(&[0x55, 0x04, 0x03]), "2.5.4.3");
        assert_eq!(dotted_oid(&[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x01]), "1.2.840.113549.1.9.1");
        assert_eq!(dotted_oid(&[0x09, 0x92, 0x26, 0x89, 0x93, 0xF2, 0x2C, 0x64, 0x01, 0x19]), "0.9.2342.19200300.100.1.25");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(subject(&[]).is_err());
        assert!(subject(&[0x30, 0x84]).is_err());
        assert!(subject(&[0x04, 0x01, 0x00]).is_err());
    }
}
