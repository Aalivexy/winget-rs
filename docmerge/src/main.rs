use std::collections::HashMap;
use std::fs;

fn main() {
    let idl_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("PackageManager.idl");
    let idl = fs::read_to_string(&idl_path)
        .unwrap_or_else(|_| panic!("failed to read IDL file: {}", idl_path.display()));

    let docs = parse_idl(&idl);
    println!("Parsed {} doc entries from IDL", docs.len());

    let rust = fs::read_to_string("src/bindings.rs")
        .expect("failed to read src/bindings.rs (run `cargo run -p bindgen` first?)");
    let result = inject(&rust, &docs);
    fs::write("src/bindings.rs", &result).expect("failed to write src/bindings.rs");
    println!("Written annotated bindings to src/bindings.rs");
}

fn parse_idl(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let mut doc_lines: Vec<String> = Vec::new();

        while i < lines.len() {
            let t = lines[i].trim();
            if t.starts_with("///") {
                let text = t.trim_start_matches('/').trim_end().to_string();
                if !text.is_empty() && !is_note(&text) {
                    doc_lines.push(text);
                }
                i += 1;
            } else {
                break;
            }
        }

        while i < lines.len() && lines[i].trim().starts_with('[') {
            i += 1;
        }
        if i >= lines.len() {
            break;
        }

        let line = lines[i].trim();

        if let Some(name) = type_name(line) {
            let type_key = name.to_string();
            if !doc_lines.is_empty() {
                map.entry(type_key.clone())
                    .or_insert_with(|| doc_lines.join("\n"));
            }

            if line.contains('{') || line.ends_with('{') || {
                i + 1 < lines.len() && lines[i + 1].trim() == "{"
            } {
                if !line.contains('{') && !line.ends_with('{') {
                    i += 1;
                }
                i += 1;
                let mut depth = 1;
                let mut member_docs: Vec<String> = Vec::new();

                while i < lines.len() && depth > 0 {
                    let t = lines[i].trim();
                    depth += t.matches('{').count();
                    depth -= t.matches('}').count();
                    if depth == 0 {
                        i += 1;
                        break;
                    }

                    if t.starts_with("///") {
                        let text = t.trim_start_matches('/').trim_end().to_string();
                        if !text.is_empty() && !is_note(&text) {
                            member_docs.push(text);
                        }
                        i += 1;
                        continue;
                    }
                    if t.starts_with("// ") || t.is_empty() {
                        i += 1;
                        continue;
                    }
                    if t.starts_with('[') {
                        i += 1;
                        continue;
                    }

                    if let Some(mname) = member_name(t) {
                        let key = format!("{}.{}", type_key, mname);
                        if !member_docs.is_empty() {
                            map.entry(key).or_insert_with(|| member_docs.join("\n"));
                        }
                        member_docs.clear();
                    }
                    i += 1;
                }
                continue;
            }
        }
        i += 1;
    }

    map
}

fn is_note(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("IMPLEMENTATION NOTE")
        || text.starts_with("DESIGN NOTE")
        || text.starts_with("USAGE NOTE")
}

fn type_name(line: &str) -> Option<&str> {
    let line = line
        .trim_end_matches('{')
        .trim()
        .trim_end_matches(';')
        .trim();
    for kw in &["enum ", "struct ", "runtimeclass ", "delegate "] {
        if let Some(rest) = line.strip_prefix(kw) {
            return rest.split_whitespace().next();
        }
    }
    None
}

fn member_name(line: &str) -> Option<String> {
    let line = line
        .trim_end_matches(';')
        .trim()
        .trim_end_matches(',')
        .trim();
    if line.is_empty() || line.starts_with('[') {
        return None;
    }

    if let Some(paren) = line.find('(') {
        let before = line[..paren].trim();
        return before.split_whitespace().last().map(|s| s.to_string());
    }

    let brace = line.find('{').unwrap_or(line.len());
    let before = line[..brace].trim();
    let words: Vec<&str> = before.split_whitespace().collect();
    if words.len() >= 2 {
        return Some(words.last().unwrap().to_string());
    }
    None
}

fn inject(rust: &str, docs: &HashMap<String, String>) -> String {
    use syn::*;

    let mut syntax: syn::File =
        parse_file(rust).expect("failed to parse src/bindings.rs as valid Rust");
    let mut count = 0;

    fn walk_items(items: &mut [Item], docs: &HashMap<String, String>, count: &mut usize) {
        for item in items.iter_mut() {
            match item {
                Item::Struct(s) => {
                    if let Some(doc) = docs.get(&s.ident.to_string()) {
                        add_doc(&mut s.attrs, doc);
                        *count += 1;
                    }
                }
                Item::Mod(m) => {
                    if let Some((_, ref mut nested_items)) = m.content {
                        walk_items(nested_items, docs, count);
                    }
                }
                Item::Impl(imp) => {
                    let type_name = if let Type::Path(p) = &*imp.self_ty {
                        p.path
                            .segments
                            .last()
                            .map(|s| s.ident.to_string())
                            .unwrap_or_default()
                    } else {
                        continue;
                    };

                    for member in &mut imp.items {
                        let member_name = match member {
                            ImplItem::Fn(m) => m.sig.ident.to_string(),
                            ImplItem::Const(c) => c.ident.to_string(),
                            _ => continue,
                        };

                        let key = format!("{}.{}", type_name, member_name);
                        if let Some(doc) = docs.get(&key) {
                            add_doc(
                                match member {
                                    ImplItem::Fn(m) => &mut m.attrs,
                                    ImplItem::Const(c) => &mut c.attrs,
                                    _ => unreachable!(),
                                },
                                doc,
                            );
                            *count += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    walk_items(&mut syntax.items, docs, &mut count);
    eprintln!("Injected {} doc attributes", count);

    prettyplease::unparse(&syntax)
}

fn add_doc(attrs: &mut Vec<syn::Attribute>, doc: &str) {
    if attrs.iter().any(|a| a.path().is_ident("doc")) {
        return;
    }
    let escaped = doc.replace('\\', "\\\\").replace('"', "\\\"");
    let attr: syn::Attribute = syn::parse_quote!(#[doc = #escaped]);
    attrs.push(attr);
}
