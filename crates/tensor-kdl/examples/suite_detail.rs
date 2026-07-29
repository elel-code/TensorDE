fn main() {
    let root = std::path::Path::new("references/kdl/tests/test_cases");
    let mut parse_ok = 0;
    let mut should = 0;
    let mut rt_ok = 0;
    let mut rt_bad = vec![];
    let mut unexp = vec![];
    let mut fa = vec![];
    let mut rej = 0;
    let mut should_fail = 0;
    for e in std::fs::read_dir(root.join("input")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("kdl") {
            continue;
        }
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&p).unwrap();
        let exp_path = root.join("expected_kdl").join(format!("{name}.kdl"));
        if exp_path.is_file() {
            should += 1;
            match tensor_kdl::from_str(&src) {
                Ok(doc) => {
                    parse_ok += 1;
                    let got = normalize(&tensor_kdl::format_document(&doc));
                    let exp = normalize(&std::fs::read_to_string(&exp_path).unwrap());
                    if got == exp {
                        rt_ok += 1;
                    } else {
                        rt_bad.push(name);
                    }
                }
                Err(e) => unexp.push(format!("{name}: {e}")),
            }
        } else {
            should_fail += 1;
            match tensor_kdl::from_str(&src) {
                Ok(_) => fa.push(name),
                Err(_) => rej += 1,
            }
        }
    }
    println!(
        "parse {parse_ok}/{should} rt {rt_ok}/{should} reject {rej}/{should_fail} FA {} UF {}",
        fa.len(),
        unexp.len()
    );
    for u in &unexp {
        println!("UF {u}");
    }
    for n in rt_bad.iter().take(10) {
        println!("RT {n}");
    }
}
fn normalize(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().map(|l| l.trim_end()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut o = lines.join("\n");
    if !o.is_empty() {
        o.push('\n');
    }
    o
}
