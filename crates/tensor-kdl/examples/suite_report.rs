fn main() {
    let root = std::path::Path::new("references/kdl/tests/test_cases");
    let mut false_accept = vec![];
    let mut unexpected = vec![];
    for e in std::fs::read_dir(root.join("input")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) != Some("kdl") {
            continue;
        }
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&p).unwrap();
        let expect = root.join("expected_kdl").join(format!("{name}.kdl"));
        let should_parse = expect.is_file();
        match tensor_kdl::from_str(&src) {
            Ok(_) if !should_parse => false_accept.push(name),
            Err(e) if should_parse => unexpected.push(format!("{name}: {e}")),
            _ => {}
        }
    }
    false_accept.sort();
    unexpected.sort();
    println!("FALSE_ACCEPT {}", false_accept.len());
    for n in &false_accept {
        println!("FA {n}");
    }
    println!("UNEXPECTED {}", unexpected.len());
    for n in &unexpected {
        println!("UF {n}");
    }
}
