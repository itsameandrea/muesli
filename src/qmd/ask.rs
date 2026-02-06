use crate::config::loader::load_config;
use crate::error::{MuesliError, Result};

pub async fn ask(question: &str) -> Result<()> {
    let config = load_config()?;

    if !config.qmd.enabled {
        eprintln!("qmd search is not enabled. Run 'muesli setup' to configure.");
        return Ok(());
    }

    println!("Searching meeting notes...\n");

    let search_results =
        crate::qmd::search::search(question, &config.qmd.collection_name, 5, false)?;

    if search_results.trim().is_empty() {
        println!("No relevant meeting notes found for: {}", question);
        return Ok(());
    }

    if config.llm.provider == "none" {
        println!("Relevant meeting notes:\n");
        println!("{}", search_results);
        println!("\n---");
        println!("Tip: Configure an LLM provider to get AI-powered answers.");
        println!("     Run: muesli setup (step 7)");
        return Ok(());
    }

    println!("Asking LLM...\n");

    let prompt = format!(
        "Based on the following meeting notes, answer this question concisely: {}\n\n\
         Meeting Notes Context:\n{}\n\n\
         Provide a clear, direct answer. Reference which meeting(s) the information comes from.",
        question, search_results
    );

    let answer = crate::llm::ask(&config.llm, &prompt)
        .await
        .map_err(|e| MuesliError::Qmd(format!("LLM error: {}", e)))?;

    println!("{}", answer);

    Ok(())
}
