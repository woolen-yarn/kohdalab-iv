use crate::tests::GPIB_TEST_LOCK as TEST_LOCK;
use crate::{DevicePair, HelperPaths, Role};
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::PathBuf};
fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}
fn fixture(name: &str) -> (HelperPaths, PathBuf) {
    let dir = std::env::temp_dir().join(format!("iv-model-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let log = dir.join("requests.log");
    let _ = fs::remove_file(&log);
    let file = dir.join("helper.sh");
    let script = format!(
        r#"#!/bin/sh
out=16
printf 'KIV\tOK\t\n'
while read op address timeout data; do
 printf '%s %s %s\n' "$op" "$address" "$data" >> '{}'
 case "$op:$address:$data" in
 QUERY:3:{os}) printf 'KIV\tOK\t{panel}\n';;
 QUERY:7:{idn}) printf 'KIV\tOK\t{adc}\n';;
 QUERY:3:{oc}) if [ "$out" = 16 ]; then printf 'KIV\tOK\t{on}\n'; else printf 'KIV\tOK\t{off}\n'; fi;;
 QUERY:3:{od}) printf 'KIV\tOK\t{zero}\n';;
 QUERY:7:{read}) printf 'KIV\tOK\t{reading}\n';;
 QUERY:7:{err}) printf 'KIV\tOK\t{noerr}\n';;
 WRITE:3:{outoff}) out=0;printf 'KIV\tOK\t\n';;
 WRITE:3:{outon}) out=16;printf 'KIV\tOK\t\n';;
 WRITE:*:*|LOCAL:*:*|REN:*:*|CLEAR:*:*) printf 'KIV\tOK\t\n';;
 EXIT:*:*) printf 'KIV\tOK\t\n';exit;;
 *) printf 'KIV\tERR\t{bad}\n';;
 esac
done
"#,
        log.display(),
        os = hex("OS;E\n"),
        panel = hex("7651 REV1\nF1R2S+0.0000E0\nT0\nLV30LA5\nEND\n"),
        idn = hex("*IDN?\n"),
        adc = hex("ADCMT,AD7461A,SERIAL,1\n"),
        oc = hex("OC;E\n"),
        od = hex("OD;E\n"),
        on = hex("STS1=16\n"),
        off = hex("STS1=0\n"),
        zero = hex("0\n"),
        read = hex("READ?\n"),
        reading = hex("+1.25E-6\n"),
        err = hex(":SYSTem:ERRor?\n"),
        noerr = hex("0,No error\n"),
        outoff = hex("O0;E\n"),
        outon = hex("O1;E\n"),
        bad = hex("Unexpected instrument command")
    );
    fs::write(&file, script).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
    (
        HelperPaths {
            gpib: file.clone(),
            usbtmc: file,
            setup: None,
        },
        log,
    )
}
#[test]
fn legacy_source_and_adc_follow_the_python_protocol_and_verified_shutdown() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (paths, log) = fixture("protocol");
    let mut p = DevicePair::new(&paths);
    p.connect_model(Role::Source, "GPIB0::3::INSTR", "YOKOGAWA_7651")
        .unwrap();
    p.connect_model(Role::Meter, "GPIB0::7::INSTR", "ADCMT_7461A")
        .unwrap();
    assert_eq!(p.query(Role::Source, "*IDN?").unwrap(), "7651 REV1");
    assert_eq!(p.query(Role::Source, ":SOUR:FUNC?").unwrap(), "VOLT");
    assert_eq!(p.query(Role::Source, ":SOUR:RANG?").unwrap(), "2");
    p.force_output_off().unwrap();
    p.write(Role::Source, ":SOUR:FUNC VOLT").unwrap();
    p.write(Role::Source, ":SOUR:RANG 2").unwrap();
    p.write(Role::Source, ":SOUR:PROT:CURR 0.005").unwrap();
    p.write(Role::Meter, "CONF:CURR:DC").unwrap();
    p.write(Role::Meter, "SENS:CURR:DC:NPLC 1").unwrap();
    p.write(Role::Source, ":SOUR:LEV:FIX 1e-3").unwrap();
    assert_eq!(p.query(Role::Meter, "READ?").unwrap(), "+1.25E-6");
    p.write(Role::Source, ":OUTP ON").unwrap();
    assert_eq!(p.query(Role::Source, ":OUTP?").unwrap(), "1");
    assert!(p.force_output_off().unwrap().output_off_verified);
    assert_eq!(p.query(Role::Source, ":OUTP?").unwrap(), "0");
    p.close().unwrap();
    let text = fs::read_to_string(log).unwrap();
    assert_eq!(
        text.matches(&format!("QUERY 7 {}", hex("READ?\n"))).count(),
        2,
        "first ADC reading must be discarded"
    );
    assert!(text.contains(&hex("LA5;E\n")));
    assert!(text.contains(&hex(":SENSE:CURRENT:DC:SRATE SLOW\n")));
    assert!(!text.contains(&format!("WRITE 3 {}", hex(":OUTP OFF\n"))));
    assert!(!text.contains(&hex("SYST:LOC\n")));
}
#[test]
fn wrong_meter_model_is_rejected_before_configuration() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (paths, log) = fixture("identity");
    let mut p = DevicePair::new(&paths);
    assert!(
        p.connect_model(Role::Meter, "GPIB0::7::INSTR", "AGILENT_34401A")
            .is_err()
    );
    assert!(!p.is_connected(Role::Meter));
    let calls = fs::read_to_string(log).unwrap();
    assert!(!calls.contains("WRITE "));
}

#[test]
fn adc_usb_uses_delayed_raw_reads_and_adc_commands_via_official_visa() {
    let _guard = TEST_LOCK.lock().unwrap();
    let (paths, log) = fixture("adc-usb");
    let helper = paths.gpib.parent().unwrap().join("kohdalab-visa-helper");
    let script = format!(
        r#"#!/bin/sh
last=''
printf 'KIV\tOK\t\n'
while read op address timeout data;do
 printf '%s %s %s\n' "$op" "$address" "$data" >> '{}'
 case "$op:$data" in
 QUERY:{idn}) printf 'KIV\tOK\t{identity}\n';;
 WRITE:{err}) last=error;printf 'KIV\tOK\t\n';;
 READ:-) if [ "$last" = error ];then printf 'KIV\tOK\t{noerr}\n';last='';else printf 'KIV\tOK\t{reading}\n';fi;;
 WRITE:*|CLEAR:*|LOCAL:*|REN:*) printf 'KIV\tOK\t\n';;
 EXIT:*) printf 'KIV\tOK\t\n';exit;;
 *) printf 'KIV\tERR\t{bad}\n';;
 esac
done
"#,
        log.display(),
        idn = hex("*IDN?\n"),
        identity = hex("ADC,7461A,SERIAL,1\n"),
        err = hex("ERR?\n"),
        noerr = hex("0,No error\n"),
        reading = hex("NDCV1.23E-3\n"),
        bad = hex("Wrong ADC USB command")
    );
    fs::write(&helper, script).unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut pair = DevicePair::new(&paths);
    pair.connect_model(
        Role::Meter,
        "VISA::USB0::0x133E::0x0001::ADCUSBTEST::INSTR",
        "ADCMT_7461A",
    )
    .unwrap();
    pair.write(Role::Meter, "CONF:VOLT:DC").unwrap();
    pair.write(Role::Meter, "SENS:VOLT:DC:NPLC 1").unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        pair.query(Role::Meter, "SYST:ERR?").unwrap().trim(),
        "0,No error"
    );
    assert!(start.elapsed() >= std::time::Duration::from_millis(20));
    assert_eq!(pair.query(Role::Meter, "READ?").unwrap(), "1.23E-3");
    pair.disconnect(Role::Meter).unwrap();
    let requests = fs::read_to_string(log).unwrap();
    assert!(requests.contains(&format!("WRITE 0 {}", hex("ITP1.000000000000e0\n"))));
    assert!(requests.contains(&format!("WRITE 0 {}\nREAD 0 -", hex("ERR?\n"))));
    assert_eq!(requests.matches("READ 0 -").count(), 3);
    assert!(!requests.contains(&format!("QUERY 0 {}", hex("READ?\n"))));
}
