// Stateful fake GS210 protection registers. The fixtures never open hardware.
fn with_source_protection(script: String, operation: &str, dir: &std::path::Path) -> String {
    let status = dir.join("source-condition");
    let after_read = dir.join("source-condition-after-read");
    let status_error = dir.join("source-condition-error");
    let protection_mismatch = dir.join("source-protection-mismatch");
    let prefix = format!(r#" response=''
 case "${operation}:$payload" in
 WRITE:3a4f555450204f4e0a) meter_reads=0 ;;
 WRITE:3a534f55523a50524f543a4355525220*) current_protection=${{payload#3a534f55523a50524f543a4355525220}}; current_protection=${{current_protection%0a}} ;;
 WRITE:3a534f55523a50524f543a564f4c5420*) voltage_protection=${{payload#3a534f55523a50524f543a564f4c5420}}; voltage_protection=${{voltage_protection%0a}} ;;
 WRITE:3a534f55523a52414e4720*) source_range_written=${{payload#3a534f55523a52414e4720}}; source_range_written=${{source_range_written%0a}} ;;
 QUERY:3a534f55523a52414e473f0a)
   if [ -f '{real_range}' ] && [ -n "$source_range_written" ]; then printf 'KIV\tOK\t%s\n' "$source_range_written"; continue; fi ;;
 QUERY:524541443f0a) meter_reads=$((${{meter_reads:-0}} + 1)) ;;
 QUERY:3a534f55523a50524f543a435552523f0a)
   if [ -f '{mismatch}' ]; then result=30; else result=${{current_protection:-302e32}}; fi
   printf 'KIV\tOK\t%s\n' "$result"; continue ;;
 QUERY:3a534f55523a50524f543a564f4c543f0a)
   if [ -f '{mismatch}' ]; then result=30; else result=${{voltage_protection:-31}}; fi
   printf 'KIV\tOK\t%s\n' "$result"; continue ;;
 QUERY:3a535441543a434f4e443f0a)
   if [ -f '{error}' ]; then printf 'KIV\tERR\t696e6a656374656420737461747573206661696c757265\n'; continue; fi
   if [ -f '{after}' ] && [ "${{meter_reads:-0}}" -gt 0 ]; then result=$(cat '{after}');
   elif [ -f '{status}' ]; then result=$(cat '{status}'); else result=323536; fi
   printf 'KIV\tOK\t%s\n' "$result"; continue ;;
 esac
"#,
        real_range=dir.join("use-real-source-range").display(),
        mismatch=protection_mismatch.display(),error=status_error.display(),
        after=after_read.display(),status=status.display());
    assert!(script.contains(" response=''\n"));
    script.replace(" response=''\n", &prefix)
}
