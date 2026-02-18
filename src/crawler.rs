/// Crawler module — autonomous corpus building via citation graph traversal.
///
/// Extracts references from ingested documents, creates reference nodes and
/// citation edges, enabling downstream crawl-and-ingest of the citation frontier.
use pgrx::prelude::*;
use regex::Regex;
use serde_json::json;
use std::collections::HashMap;

use crate::parser::kinds::Kind;
use crate::sql::{sql_jsonb, sql_text, sql_uuid};

/// A reference extracted from document text.
#[derive(Debug, Clone)]
struct ExtractedRef {
    /// Canonical key for deduplication (e.g. "friston-2010" or DOI).
    #[allow(dead_code)]
    key: String,
    /// Display content for the reference node.
    content: String,
    /// Reference type: "bibliography", "citation", "doi", "arxiv", "url".
    ref_type: String,
    /// Structured metadata.
    metadata: serde_json::Value,
    /// Paragraph node IDs that cite this reference.
    citing_paragraphs: Vec<String>,
}

/// Extract references from all documents, or a specific document.
///
/// Scans paragraph nodes for bibliography entries, inline author-year citations,
/// DOIs, arXiv IDs, and URLs. Creates `reference` nodes and `cites` edges.
/// Deduplicates by canonical key (DOI > author-year > URL).
///
/// Returns a JSON summary of what was found.
#[pg_extern]
fn extract_references(document_filter: default!(&str, "'__all__'")) -> pgrx::JsonB {
    let instance_id = crate::parser::get_self_instance_id();

    // Query paragraph nodes from matching documents
    let doc_clause = if document_filter == "__all__" {
        String::new()
    } else {
        format!(
            " AND doc.content = {}",
            sql_text(document_filter)
        )
    };

    let query = format!(
        "SELECT p.id::text, p.content, doc.id::text as doc_id, doc.content as doc_name \
         FROM kerai.nodes p \
         JOIN kerai.nodes heading ON heading.id = p.parent_id \
         JOIN kerai.nodes doc ON doc.id = heading.parent_id \
         WHERE p.kind = 'paragraph' \
         AND doc.kind = 'document' \
         AND length(p.content) > 50 \
         {}
         ORDER BY doc.content, p.position",
        doc_clause
    );

    let mut refs: HashMap<String, ExtractedRef> = HashMap::new();
    let mut total_paragraphs = 0u64;

    Spi::connect(|client| {
        let result = client.select(&query, None, &[]).unwrap();
        for row in result {
            let para_id: String = row.get_by_name("id").unwrap().unwrap_or_default();
            let content: String = row.get_by_name("content").unwrap().unwrap_or_default();
            total_paragraphs += 1;

            // Extract bibliography entries
            extract_bibliography(&content, &para_id, &mut refs);

            // Extract inline citations
            extract_inline_citations(&content, &para_id, &mut refs);

            // Extract DOIs
            extract_dois(&content, &para_id, &mut refs);

            // Extract arXiv IDs
            extract_arxiv(&content, &para_id, &mut refs);

            // Extract URLs
            extract_urls(&content, &para_id, &mut refs);
        }
    });

    // Check which references already exist (idempotent)
    let existing = get_existing_reference_keys(&instance_id);

    // Insert new reference nodes and citation edges
    let mut new_refs = 0u64;
    let mut new_edges = 0u64;
    let mut skipped = 0u64;

    for (key, ext_ref) in &refs {
        if existing.contains_key(key) {
            // Reference exists — just add any new citation edges
            let ref_id = &existing[key];
            new_edges += insert_citation_edges(ref_id, &ext_ref.citing_paragraphs);
            skipped += 1;
            continue;
        }

        let ref_id = uuid::Uuid::new_v4().to_string();

        let meta = json!({
            "ref_type": ext_ref.ref_type,
            "status": "unresolved",
            "cite_count": ext_ref.citing_paragraphs.len(),
            "key": key,
            "details": ext_ref.metadata,
        });

        let insert_sql = format!(
            "INSERT INTO kerai.nodes (id, instance_id, kind, content, metadata, position) \
             VALUES ({}, {}, {}, {}, {}, 0)",
            sql_uuid(&ref_id),
            sql_uuid(&instance_id),
            sql_text(Kind::Reference.as_str()),
            sql_text(&ext_ref.content),
            sql_jsonb(&meta),
        );

        Spi::run(&insert_sql).ok();
        new_refs += 1;

        new_edges += insert_citation_edges(&ref_id, &ext_ref.citing_paragraphs);
    }

    pgrx::JsonB(json!({
        "paragraphs_scanned": total_paragraphs,
        "references_found": refs.len(),
        "new_references": new_refs,
        "new_edges": new_edges,
        "skipped_existing": skipped,
    }))
}

/// Extract bibliography entries: `Author, F. N. (YYYY). "Title." Journal ...`
fn extract_bibliography(text: &str, para_id: &str, refs: &mut HashMap<String, ExtractedRef>) {
    // Bibliography paragraphs contain multiple entries concatenated.
    // Pattern: starts with author surname, contains (year) and quoted title.
    let re = Regex::new(
        r#"([A-Z][a-z\x{00AD}\x{00C0}-\x{024F}]+(?:,\s*(?:[A-Z]\.\s*)+(?:and\s+)?)*(?:,?\s*(?:et\s+al\.))?)\s*\((\d{4}[a-z]?)\)\.\s*["\x{201C}]([^"\x{201D}]+)["\x{201D}]"#
    ).unwrap();

    for cap in re.captures_iter(text) {
        let authors = cap[1].trim().to_string();
        let year = cap[2].to_string();
        let title = cap[3].trim().to_string();

        // Extract DOI if present in this entry
        let doi = extract_doi_from_segment(text, cap.get(0).unwrap().start());

        let key = if let Some(ref d) = doi {
            d.clone()
        } else {
            make_author_year_key(&authors, &year)
        };

        let content = format!("{} ({}). \"{}\"", authors, year, title);

        let entry = refs.entry(key.clone()).or_insert_with(|| ExtractedRef {
            key,
            content,
            ref_type: "bibliography".to_string(),
            metadata: json!({
                "authors": authors,
                "year": year,
                "title": title,
                "doi": doi,
            }),
            citing_paragraphs: Vec::new(),
        });

        if !entry.citing_paragraphs.contains(&para_id.to_string()) {
            entry.citing_paragraphs.push(para_id.to_string());
        }
    }
}

/// Extract inline citations: `(Author YYYY)`, `(Author & Author, YYYY)`, `(Author et al. YYYY)`
fn extract_inline_citations(
    text: &str,
    para_id: &str,
    refs: &mut HashMap<String, ExtractedRef>,
) {
    // Match patterns like (Friston 2010), (Parr & Friston, 2019), (Da Costa et al., 2020)
    // Also handles (Friston, FitzGerald et al. 2016)
    let re = Regex::new(
        r"\(([A-Z][a-z\x{00AD}\x{00C0}-\x{024F}]+(?:(?:,?\s*(?:and|&)\s*)?[A-Z][a-z\x{00AD}\x{00C0}-\x{024F}]+)*(?:,?\s*et\s+al\.?)?),?\s*(\d{4}[a-z]?)\)"
    ).unwrap();

    for cap in re.captures_iter(text) {
        let authors = cap[1].trim().to_string();
        let year = cap[2].to_string();
        let key = make_author_year_key(&authors, &year);

        let entry = refs.entry(key.clone()).or_insert_with(|| ExtractedRef {
            key,
            content: format!("{} ({})", authors, year),
            ref_type: "citation".to_string(),
            metadata: json!({
                "authors": authors,
                "year": year,
            }),
            citing_paragraphs: Vec::new(),
        });

        if !entry.citing_paragraphs.contains(&para_id.to_string()) {
            entry.citing_paragraphs.push(para_id.to_string());
        }
    }
}

/// Extract DOI references.
fn extract_dois(text: &str, para_id: &str, refs: &mut HashMap<String, ExtractedRef>) {
    let re = Regex::new(r"doi:?(10\.\d{4,}/[^\s,;]+)").unwrap();

    for cap in re.captures_iter(text) {
        let doi = cap[1].trim_end_matches('.').to_string();
        let key = doi.clone();

        let entry = refs.entry(key.clone()).or_insert_with(|| ExtractedRef {
            key,
            content: format!("doi:{}", doi),
            ref_type: "doi".to_string(),
            metadata: json!({ "doi": doi }),
            citing_paragraphs: Vec::new(),
        });

        if !entry.citing_paragraphs.contains(&para_id.to_string()) {
            entry.citing_paragraphs.push(para_id.to_string());
        }
    }
}

/// Extract arXiv IDs.
fn extract_arxiv(text: &str, para_id: &str, refs: &mut HashMap<String, ExtractedRef>) {
    let re = Regex::new(r"arXiv[:\s]+([\d.]+(?:v\d+)?(?:\s*\[[a-z.,\s]+\])?)").unwrap();

    for cap in re.captures_iter(text) {
        let arxiv_id = cap[1].trim().to_string();
        // Clean up: take just the numeric ID part
        let clean_id = arxiv_id
            .split_whitespace()
            .next()
            .unwrap_or(&arxiv_id)
            .trim_end_matches('.')
            .to_string();
        let key = format!("arxiv:{}", clean_id);

        let entry = refs.entry(key.clone()).or_insert_with(|| ExtractedRef {
            key,
            content: format!("arXiv:{}", clean_id),
            ref_type: "arxiv".to_string(),
            metadata: json!({ "arxiv_id": clean_id }),
            citing_paragraphs: Vec::new(),
        });

        if !entry.citing_paragraphs.contains(&para_id.to_string()) {
            entry.citing_paragraphs.push(para_id.to_string());
        }
    }
}

/// Extract URLs.
fn extract_urls(text: &str, para_id: &str, refs: &mut HashMap<String, ExtractedRef>) {
    let re = Regex::new(r"https?://[^\s,;)\]]+").unwrap();

    for mat in re.find_iter(text) {
        let url = mat.as_str().trim_end_matches('.').to_string();
        let key = url.clone();

        let entry = refs.entry(key.clone()).or_insert_with(|| ExtractedRef {
            key,
            content: url.clone(),
            ref_type: "url".to_string(),
            metadata: json!({ "url": url }),
            citing_paragraphs: Vec::new(),
        });

        if !entry.citing_paragraphs.contains(&para_id.to_string()) {
            entry.citing_paragraphs.push(para_id.to_string());
        }
    }
}

/// Try to find a DOI near a position in the text (for bibliography entries).
fn extract_doi_from_segment(text: &str, start: usize) -> Option<String> {
    // Look ahead up to 500 chars from the match start for a doi.
    // Must find a valid char boundary to avoid panicking on multi-byte UTF-8.
    let end = {
        let target = (start + 500).min(text.len());
        // Walk backward to find a char boundary
        let mut pos = target;
        while pos > start && !text.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    };
    if end <= start {
        return None;
    }
    let segment = &text[start..end];
    let re = Regex::new(r"doi:?(10\.\d{4,}/[^\s,;]+)").unwrap();
    re.captures(segment)
        .map(|c| c[1].trim_end_matches('.').to_string())
}

/// Create a canonical author-year key for deduplication.
fn make_author_year_key(authors: &str, year: &str) -> String {
    // Extract first surname, lowercase, strip diacritics/hyphens
    let first_author = authors
        .split(',')
        .next()
        .unwrap_or(authors)
        .split_whitespace()
        .next()
        .unwrap_or(authors)
        .to_lowercase()
        .replace('\u{00AD}', "") // soft hyphen
        .replace('\u{00A0}', "") // nbsp
        .replace('-', "");

    format!("{}-{}", first_author, year)
}

/// Get existing reference nodes by key to avoid duplicates.
fn get_existing_reference_keys(instance_id: &str) -> HashMap<String, String> {
    let mut existing = HashMap::new();

    Spi::connect(|client| {
        let query = format!(
            "SELECT id::text, metadata->>'key' as key \
             FROM kerai.nodes \
             WHERE instance_id = {} AND kind = 'reference' \
             AND metadata->>'key' IS NOT NULL",
            sql_uuid(instance_id)
        );
        let result = client.select(&query, None, &[]).unwrap();
        for row in result {
            let id: String = row.get_by_name("id").unwrap().unwrap_or_default();
            let key: String = row.get_by_name("key").unwrap().unwrap_or_default();
            if !key.is_empty() {
                existing.insert(key, id);
            }
        }
    });

    existing
}

/// Insert citation edges from paragraphs to a reference node.
/// Returns the number of new edges created.
fn insert_citation_edges(ref_id: &str, para_ids: &[String]) -> u64 {
    let mut count = 0u64;

    for para_id in para_ids {
        // Check if edge already exists
        let exists_sql = format!(
            "SELECT 1 FROM kerai.edges \
             WHERE source_id = {} AND target_id = {} AND relation = 'cites'",
            sql_uuid(para_id),
            sql_uuid(ref_id)
        );

        let exists = Spi::get_one::<i32>(&exists_sql)
            .unwrap_or(None)
            .is_some();

        if !exists {
            let edge_id = uuid::Uuid::new_v4().to_string();
            let insert_sql = format!(
                "INSERT INTO kerai.edges (id, source_id, target_id, relation, metadata) \
                 VALUES ({}, {}, {}, 'cites', '{{}}'::jsonb)",
                sql_uuid(&edge_id),
                sql_uuid(para_id),
                sql_uuid(ref_id),
            );
            Spi::run(&insert_sql).ok();
            count += 1;
        }
    }

    count
}
