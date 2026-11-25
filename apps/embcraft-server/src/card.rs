//! The files for a new design stick's card: copied onto the boot partition (bootfs) of a card just
//! written with Raspberry Pi OS Lite, they make the Pi join the Wi-Fi, take its name, let the
//! library in with its key, and install the stick software (station/pi-stick) on its first start.

use serde::Deserialize;
use serde_json::json;

/// The stick software, built into the library.
const KIT: [(&str, &[u8]); 10] = [
    (
        "ecstick",
        include_bytes!("../../../station/pi-stick/ecstick"),
    ),
    (
        "ecstick.conf",
        include_bytes!("../../../station/pi-stick/ecstick.conf"),
    ),
    (
        "ecstick.service",
        include_bytes!("../../../station/pi-stick/ecstick.service"),
    ),
    (
        "setup.sh",
        include_bytes!("../../../station/pi-stick/setup.sh"),
    ),
    (
        "first-start.sh",
        include_bytes!("../../../station/pi-stick/first-start.sh"),
    ),
    (
        "send.sh",
        include_bytes!("../../../station/pi-stick/send.sh"),
    ),
    (
        "wifi-report.sh",
        include_bytes!("../../../station/pi-stick/wifi-report.sh"),
    ),
    (
        "wifi-report.service",
        include_bytes!("../../../station/pi-stick/wifi-report.service"),
    ),
    (
        "README.md",
        include_bytes!("../../../station/pi-stick/README.md"),
    ),
    (
        "trimtest.dst",
        include_bytes!("../../../station/pi-stick/trimtest.dst"),
    ),
];

#[derive(Debug, Deserialize)]
pub struct Request {
    /// The stick's network name: ecstick2 is reached as ecstick2.local.
    pub name: String,
    pub ssid: String,
    pub wifi_password: String,
    #[serde(default = "country")]
    pub country: String,
    #[serde(default = "zone")]
    pub timezone: String,
    #[serde(default = "user")]
    pub user: String,
    /// More public keys let in (other computers), one per line.
    #[serde(default)]
    pub keys: String,
}

fn country() -> String {
    "CA".into()
}
fn zone() -> String {
    "America/Toronto".into()
}
fn user() -> String {
    "shop".into()
}

fn check(r: &Request) -> Result<(), String> {
    let name_ok = !r.name.is_empty()
        && r.name.len() <= 63
        && r.name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !r.name.starts_with('-')
        && !r.name.ends_with('-');
    if !name_ok {
        return Err(
            "The stick's name: lower-case letters, digits and - only (e.g. ecstick2).".into(),
        );
    }
    if r.ssid.is_empty() || r.ssid.len() > 32 {
        return Err("Give the Wi-Fi's name (at most 32 characters).".into());
    }
    let pw = r.wifi_password.len();
    if !(8..=63).contains(&pw) || r.wifi_password.contains(['\n', '\r']) {
        return Err("The Wi-Fi password has 8 to 63 characters.".into());
    }
    if r.country.len() != 2 || !r.country.chars().all(|c| c.is_ascii_uppercase()) {
        return Err("The country: two capital letters, e.g. CA.".into());
    }
    if r.timezone.is_empty()
        || !r
            .timezone
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))
    {
        return Err("Unknown time zone.".into());
    }
    let user_ok = r.user.len() <= 32
        && r.user.starts_with(|c: char| c.is_ascii_lowercase())
        && r.user
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !user_ok {
        return Err("The user name: lower-case letters and digits.".into());
    }
    Ok(())
}

/// The other keys, checked: each line one public key.
fn keys(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let kind = line.split_whitespace().next().unwrap_or("");
        let known =
            kind.starts_with("ssh-") || kind.starts_with("ecdsa-") || kind.starts_with("sk-");
        if !known || line.split_whitespace().count() < 2 {
            return Err(format!(
                "Not a public key: \"{}…\"",
                line.chars().take(30).collect::<String>()
            ));
        }
        out.push(line.to_string());
    }
    Ok(out)
}

/// A value in a NetworkManager keyfile: a backslash escapes, and edge spaces must be escaped.
fn nm_value(text: &str) -> String {
    let mut t = text.replace('\\', "\\\\");
    if let Some(rest) = t.strip_prefix(' ') {
        t = format!("\\s{rest}");
    }
    if let Some(rest) = t.strip_suffix(' ') {
        t = format!("{rest}\\s");
    }
    t
}

/// The Wi-Fi profile, straight for NetworkManager (Pi OS's cloud-init route through netplan can
/// leave an empty one, and the Pi never joins).
fn wifi_profile(ssid: &str, psk: &str, uuid: &str) -> String {
    let ssid_line = if ssid.contains([';', '\n', '\r']) {
        ssid.bytes().map(|b| format!("{b};")).collect::<String>()
    } else {
        nm_value(ssid)
    };
    format!(
        "[connection]\nid=shop-wifi\nuuid={uuid}\ntype=wifi\ninterface-name=wlan0\nautoconnect=true\n\n\
         [wifi]\nmode=infrastructure\nssid={ssid_line}\n\n\
         [wifi-security]\nkey-mgmt=wpa-psk\npsk={}\n\n\
         [ipv4]\nmethod=auto\n\n[ipv6]\nmethod=auto\n",
        nm_value(psk)
    )
}

/// A random-looking UUID (version 4 layout) from the time and the name.
fn uuid(seed: &str) -> String {
    use sha2::{Digest, Sha256};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let h = Sha256::digest(format!("{seed}{nanos}{:?}", std::thread::current().id()));
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let x: String = b.iter().map(|v| format!("{v:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &x[..8],
        &x[8..12],
        &x[12..16],
        &x[16..20],
        &x[20..]
    )
}

/// cloud-init's settings. JSON strings are YAML strings, so whatever was typed is quoted safely.
fn user_data(r: &Request, keys: &[String], profile: &str) -> String {
    let q = |s: &str| json!(s).to_string();
    let mut k = String::new();
    for key in keys {
        k.push_str(&format!("\n  - {}", q(key)));
    }
    format!(
        "#cloud-config
# Design stick {name}, prepared by the EmbroideryCraft Library. Holds the Wi-Fi password until the
# Pi's first start, which takes it out.
hostname: {qname}
manage_etc_hosts: true
timezone: {zone}
keyboard:
  model: pc105
  layout: us
users:
- name: {user}
  groups: users,adm,dialout,audio,netdev,video,plugdev,cdrom,games,input,gpio,spi,i2c,render,sudo
  shell: /bin/bash
  lock_passwd: true
  sudo: ALL=(ALL) NOPASSWD:ALL
  ssh_authorized_keys:{k}
enable_ssh: true
ssh_pwauth: false
write_files:
- path: /etc/NetworkManager/system-connections/shop-wifi.nmconnection
  owner: root:root
  permissions: '0600'
  content: {profile}
# install the design stick, then restart into it
runcmd:
- [bash, /boot/firmware/pi-stick/first-start.sh, {country}, {qname}]
power_state:
  mode: reboot
  message: Design stick installed, restarting
  condition: true
",
        name = r.name,
        qname = q(&r.name),
        zone = q(&r.timezone),
        user = q(&r.user),
        profile = q(profile),
        country = q(&r.country),
    )
}

const HOW_TO: &str = "Design stick card
=================

1. Write the card with Raspberry Pi Imager: device Raspberry Pi Zero 2 W, system
   Raspberry Pi OS (other) > Raspberry Pi OS Lite (64-bit). When Imager offers to
   customise the settings, answer No.
2. Take the card out and put it back in. A drive named bootfs appears.
3. Copy everything in this zip onto bootfs (the pi-stick folder too), replacing
   the files that are there. Eject the card.
4. Put the card in the Pi and power it (port PWR, or the machine's USB port with the
   cable in the Pi's USB port). The first start takes 3 to 5 minutes and restarts once.
5. In the Library, the Prepare a design stick window shows when the stick answers.

If it never answers: put the card back in the computer. wifi-report.txt on bootfs
shows the networks the Pi saw and why it could not join (2.4 GHz Wi-Fi only).
stick-installed.txt is there once the stick software is installed.

This card holds the Wi-Fi password until the Pi's first start removes it.
";

/// The zip to copy onto the card's bootfs, given the library's public key.
pub fn zip(r: &Request, library_key: &str) -> Result<Vec<u8>, String> {
    check(r)?;
    let mut all = vec![library_key.trim().to_string()];
    for k in keys(&r.keys)? {
        if !all.contains(&k) {
            all.push(k);
        }
    }
    let profile = wifi_profile(&r.ssid, &r.wifi_password, &uuid(&r.name));
    // a new instance id: cloud-init runs the first start again even on a card used before
    let meta = format!(
        "dsmode: local\ninstance-id: {}-{}\nlocal-hostname: {}\n",
        r.name,
        crate::db::now(),
        r.name
    );
    let mut files: Vec<(String, Vec<u8>)> = vec![
        (
            "user-data".into(),
            user_data(r, &all, &profile).into_bytes(),
        ),
        ("meta-data".into(), meta.into_bytes()),
        // the Wi-Fi is in user-data: cloud-init's own network setup stays out of the way
        (
            "network-config".into(),
            b"network:\n  config: disabled\n".to_vec(),
        ),
        (
            "DESIGN-STICK-README.txt".into(),
            HOW_TO.replace('\n', "\r\n").into_bytes(),
        ),
    ];
    for (name, bytes) in KIT {
        files.push((format!("pi-stick/{name}"), bytes.to_vec()));
    }
    Ok(write_zip(&files))
}

/// The first free ecstickN, given the sticks already in use (`shop@ecstick1.local`, …).
pub fn next_name(sticks: &[String]) -> String {
    let used: Vec<u32> = sticks
        .iter()
        .filter_map(|s| {
            let host = s.rsplit('@').next()?;
            let host = host.split('.').next()?;
            host.strip_prefix("ecstick")?.parse().ok()
        })
        .collect();
    let n = (1..).find(|n| !used.contains(n)).unwrap_or(1);
    format!("ecstick{n}")
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xedb8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// A plain zip (stored, no compression: the files are small).
fn write_zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    // 2026-01-01 00:00 in DOS form; the date does not matter on the card
    let (time, date) = (0u16, ((2026 - 1980) << 9 | 1 << 5 | 1) as u16);
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in files {
        let offset = out.len() as u32;
        let crc = crc32(bytes);
        let size = bytes.len() as u32;
        let n = name.as_bytes();
        let common = |v: &mut Vec<u8>| {
            v.extend_from_slice(&20u16.to_le_bytes()); // version needed
            v.extend_from_slice(&0x0800u16.to_le_bytes()); // names in UTF-8
            v.extend_from_slice(&0u16.to_le_bytes()); // stored
            v.extend_from_slice(&time.to_le_bytes());
            v.extend_from_slice(&date.to_le_bytes());
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&size.to_le_bytes());
            v.extend_from_slice(&(n.len() as u16).to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes()); // extra
        };
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend_from_slice(n);
        out.extend_from_slice(bytes);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&(0x0300u16 | 20).to_le_bytes()); // made by: Unix
        common(&mut central);
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        let mode: u32 = if name.ends_with(".sh") || name.ends_with("/ecstick") {
            0o100_755
        } else {
            0o100_644
        };
        central.extend_from_slice(&(mode << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(n);
    }
    let start = out.len() as u32;
    let count = files.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> Request {
        Request {
            name: "ecstick2".into(),
            ssid: "Shop Wi-Fi".into(),
            wifi_password: "a \"quoted\" pass\\word ".into(),
            country: "CA".into(),
            timezone: "America/Toronto".into(),
            user: "shop".into(),
            keys: String::new(),
        }
    }

    #[test]
    fn names() {
        assert_eq!(next_name(&[]), "ecstick1");
        let used = [
            "shop@ecstick1.local".to_string(),
            "shop@ecstick3.local".into(),
            "x@10.0.0.4".into(),
        ];
        assert_eq!(next_name(&used), "ecstick2");
    }

    #[test]
    fn checks() {
        let mut r = req();
        r.name = "EcStick".into();
        assert!(zip(&r, "ssh-ed25519 AAAA lib").is_err());
        let mut r = req();
        r.wifi_password = "short".into();
        assert!(zip(&r, "ssh-ed25519 AAAA lib").is_err());
        let mut r = req();
        r.keys = "not a key".into();
        assert!(zip(&r, "ssh-ed25519 AAAA lib").is_err());
    }

    #[test]
    fn profile_escapes() {
        let p = wifi_profile(" a;b", "pw\\x ", "u");
        assert!(p.contains("ssid=32;97;59;98;\n"), "{p}");
        assert!(p.contains("psk=pw\\\\x\\s\n"), "{p}");
        let p = wifi_profile("Shop", "secret12", "u");
        assert!(p.contains("ssid=Shop\n"));
    }

    #[test]
    fn crc() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn zip_holds_the_card() {
        let z = zip(&req(), "ssh-ed25519 AAAA lib").unwrap();
        let dir = std::env::temp_dir().join(format!("ec-card-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("card.zip");
        std::fs::write(&path, &z).unwrap();
        // the system's unzip, when there is one, must read it
        if let Ok(out) = std::process::Command::new("unzip")
            .arg("-t")
            .arg(&path)
            .output()
        {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
        // EC_CARD_OUT=path keeps a copy, to check it with the Pi image's own cloud-init
        if let Ok(out) = std::env::var("EC_CARD_OUT") {
            std::fs::write(out, &z).unwrap();
        }
        let text = String::from_utf8_lossy(&z);
        assert!(text.contains("pi-stick/ecstick"));
        assert!(text.contains("hostname: \"ecstick2\""));
        assert!(text.contains("- \"ssh-ed25519 AAAA lib\""));
        assert!(text.contains("first-start.sh, \"CA\", \"ecstick2\"]"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
