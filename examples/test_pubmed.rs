// Integration smoke test: parse_pubmed_input + fetch_pubmed_with_failures (network)

#[path = "../src/pubmed.rs"]
mod pubmed;

fn main() {
    // 1) PMID extraction
    let input = "看看这篇 https://pubmed.ncbi.nlm.nih.gov/29045013/ 还有 30053915\n以及旧链接 https://www.ncbi.nlm.nih.gov/pubmed/12345678";
    let parsed = pubmed::parse_pubmed_input(input);
    let ids = parsed.pmids;
    println!("extracted: {ids:?}");
    println!("rejected: {:?}", parsed.rejected);
    assert!(ids.contains(&"29045013".to_string()));
    assert!(ids.contains(&"30053915".to_string()));
    assert!(ids.contains(&"12345678".to_string()));

    // 2) Network fetch (best effort)
    match pubmed::fetch_pubmed_with_failures(&["31978945".to_string()]) {
        Ok(result) => {
            for a in &result.articles {
                println!("title: {}", a.title);
                println!("abstract: {} chars", a.abstract_text.chars().count());
                println!(
                    "year: {:?}, journal: {:?}, pmid: {:?}, doi: {:?}, authors: {}",
                    a.year,
                    a.journal,
                    a.pmid,
                    a.doi,
                    a.authors.len()
                );
            }
            for failure in &result.failures {
                println!("failure: PMID {} -> {}", failure.pmid, failure.reason);
            }
            assert!(!result.articles.is_empty(), "expected at least one article");
            assert!(!result.articles[0].title.is_empty());
            assert!(
                result.failures.is_empty(),
                "unexpected failures: {:?}",
                result.failures
            );
        }
        Err(e) => {
            eprintln!("fetch_pubmed_with_failures failed (network?): {e}");
        }
    }
    println!("OK");
}
