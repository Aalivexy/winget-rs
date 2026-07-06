use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use syn::*;

fn main() {
    let root = env!("CARGO_MANIFEST_DIR");
    let idl = fs::read_to_string(std::path::Path::new(root).join("PackageManager.idl"))
        .unwrap_or_else(|_| panic!("failed to read IDL file at {root}/PackageManager.idl"));

    let docs = parse_idl(&idl);
    println!("Parsed {} doc entries from IDL", docs.len());

    let rust = fs::read_to_string("src/bindings.rs")
        .expect("failed to read src/bindings.rs (run `cargo run -p bindgen` first?)");
    let result = inject(&rust, &docs);
    fs::write("src/bindings.rs", &result).expect("failed to write src/bindings.rs");
    println!("Written annotated bindings to src/bindings.rs");
}

// ── IDL doc line predicates ────────────────────────────────

fn is_note(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("IMPLEMENTATION NOTE")
        || text.starts_with("DESIGN NOTE")
        || text.starts_with("USAGE NOTE")
}

fn doc_line_text(line: &str) -> String {
    line.trim().trim_start_matches('/').trim_end().to_string()
}

// ── IDL parser ─────────────────────────────────────────────

fn parse_idl(content: &str) -> HashMap<String, String> {
    let mut docs = HashMap::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim().starts_with("// ") || lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        // Collect doc lines (///), stopping at the first non-/// line.
        let doc = collect_doc_lines(&lines, &mut i);

        // Skip attribute lines ([...]) between doc and type name.
        skip_attributes(&lines, &mut i);

        if i >= lines.len() {
            break;
        }

        let line = lines[i].trim();
        if let Some(type_name) = type_name(line) {
            let type_key = type_name.to_string();
            if !doc.is_empty() {
                docs.entry(type_key.clone()).or_insert_with(|| doc.join("\n"));
            }

            if has_body(line, lines.get(i + 1).map(|l| l.trim())) {
                // Advance past the line containing the opening brace.
                if !line.contains('{') && !line.trim_end().ends_with('{') {
                    i += 1;
                }
                i += 1;
                parse_members(&lines, &mut i, &type_key, &mut docs);
            }
        }

        i += 1;
    }

    docs
}

/// Collect consecutive `///` comment lines starting at `i`,
/// filtering out implementation/design/usages notes.
/// Advances `i` to the first non-`///` line.
fn collect_doc_lines(lines: &[&str], i: &mut usize) -> Vec<String> {
    let mut doc = Vec::new();
    while *i < lines.len() {
        let t = lines[*i].trim();
        if !t.starts_with("///") {
            break;
        }
        let text = doc_line_text(t);
        if !text.is_empty() && !is_note(&text) {
            doc.push(text);
        }
        *i += 1;
    }
    doc
}

/// Skip lines starting with `[` (attributes / contract version blocks).
fn skip_attributes(lines: &[&str], i: &mut usize) {
    while *i < lines.len() && lines[*i].trim().starts_with('[') {
        *i += 1;
    }
}

fn type_name(line: &str) -> Option<&str> {
    let line = line.trim_end_matches('{').trim().trim_end_matches(';').trim();
    for kw in &["enum ", "struct ", "runtimeclass ", "delegate "] {
        if let Some(rest) = line.strip_prefix(kw) {
            return rest.split_whitespace().next();
        }
    }
    None
}

fn has_body(line: &str, next: Option<&str>) -> bool {
    line.contains('{') || line.trim_end().ends_with('{') || next == Some("{")
}

fn parse_members(
    lines: &[&str],
    i: &mut usize,
    type_key: &str,
    docs: &mut HashMap<String, String>,
) {
    let mut member_docs = Vec::new();
    let mut depth = 1i32;

    while *i < lines.len() {
        let t = lines[*i].trim();

        depth += t.matches('{').count() as i32;
        depth -= t.matches('}').count() as i32;

        if depth <= 0 {
            return;
        }

        if t.starts_with("///") {
            let text = doc_line_text(t);
            if !text.is_empty() && !is_note(&text) {
                member_docs.push(text);
            }
            *i += 1;
            continue;
        }

        if t.starts_with("// ") || t.is_empty() || t.starts_with('[') {
            *i += 1;
            continue;
        }

        if let Some(mname) = member_name(t) {
            let key = format!("{}.{}", type_key, mname);
            if !member_docs.is_empty() {
                docs.entry(key).or_insert_with(|| member_docs.join("\n"));
            }
            member_docs.clear();
        }

        *i += 1;
    }
}

fn member_name(line: &str) -> Option<String> {
    let line = line.trim_end_matches(';').trim().trim_end_matches(',').trim();
    if line.is_empty() || line.starts_with('[') {
        return None;
    }

    let name = if let Some(paren) = line.find('(') {
        line[..paren].trim().split_whitespace().last()?.to_string()
    } else {
        let brace = line.find('{').unwrap_or(line.len());
        line[..brace].trim().split_whitespace().last()?.to_string()
    };

    // For hex values like "0x0" or "0x0001", extract the name before "=".
    if name.starts_with('0') || name == "|" {
        if let Some(eq_pos) = line.find('=') {
            line[..eq_pos].trim().split_whitespace().last().map(|s| s.to_string())
        } else {
            None
        }
    } else {
        Some(name)
    }
}

// ── Doc injector ───────────────────────────────────────────

fn inject(rust: &str, docs: &HashMap<String, String>) -> String {
    let mut syntax: File =
        parse_file(rust).expect("failed to parse src/bindings.rs as valid Rust");
    let mut matched = HashSet::new();

    walk_items(&mut syntax.items, docs, &mut matched);

    eprintln!("Injected {} doc attributes", matched.len());

    let unmatched: Vec<_> = docs.keys().filter(|k| !matched.contains(*k)).collect();
    if !unmatched.is_empty() {
        eprintln!("Unmatched doc keys ({}):", unmatched.len());
        for key in &unmatched {
            eprintln!("  {}", key);
        }
    }

    prettyplease::unparse(&syntax)
}

fn walk_items(items: &mut [Item], docs: &HashMap<String, String>, matched: &mut HashSet<String>) {
    for item in items.iter_mut() {
        match item {
            Item::Struct(s) => {
                inject_struct(s, docs, matched);
            }
            Item::Mod(m) => {
                if let Some((_, ref mut nested)) = m.content {
                    walk_items(nested, docs, matched);
                }
            }
            Item::Impl(imp) => {
                inject_impl(imp, docs, matched);
            }
            _ => {}
        }
    }
}

fn inject_struct(s: &mut ItemStruct, docs: &HashMap<String, String>, matched: &mut HashSet<String>) {
    let key = s.ident.to_string();

    if let Some(doc) = docs.get(&key) {
        add_doc(&mut s.attrs, doc);
        matched.insert(key.clone());
    }

    // WinRT structs like InstallProgress have named fields (not IUnknown wrappers).
    if let Fields::Named(ref mut fields) = s.fields {
        for field in &mut fields.named {
            if let Some(ref ident) = field.ident {
                let field_key = format!("{}.{}", key, ident);
                if let Some(doc) = docs.get(&field_key) {
                    add_doc(&mut field.attrs, doc);
                    matched.insert(field_key);
                }
            }
        }
    }
}

fn inject_impl(imp: &mut ItemImpl, docs: &HashMap<String, String>, matched: &mut HashSet<String>) {
    let type_name = match &*imp.self_ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
        _ => return,
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
            matched.insert(key);
        }
    }
}

fn add_doc(attrs: &mut Vec<Attribute>, doc: &str) {
    if attrs.iter().any(|a| a.path().is_ident("doc")) {
        return;
    }
    let escaped = doc.replace('\\', "\\\\").replace('"', "\\\"");
    let attr: Attribute = parse_quote!(#[doc = #escaped]);
    attrs.push(attr);
}
