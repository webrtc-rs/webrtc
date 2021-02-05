use super::*;

use regex::Regex;

#[test]
fn test_generate_multicast_dnsname() -> Result<(), Error> {
    let name = generate_multicast_dns_name();

    let re = Regex::new(
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}.local+$",
    );

    if let Ok(re) = re {
        assert!(
            re.is_match(&name),
            "mDNS name must be UUID v4 + \".local\" suffix, got {}",
            name
        );
    } else {
        assert!(false, "expected ok, but got err");
    }

    Ok(())
}
