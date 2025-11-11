use nexus_core::executor::parser::CypherParser;

fn main() {
    let queries = vec![
        "CREATE (n:Person)",
        "CREATE (n:Person {name: 'Alice'})",
        "MATCH (n) DETACH DELETE n",
        "MATCH (n:Person) DELETE n",
    ];
    
    for query in queries {
        println!("\nParsing: {}", query);
        let mut parser = CypherParser::new(query.to_string());
        match parser.parse() {
            Ok(ast) => {
                println!("  ✅ Success: {} clauses", ast.clauses.len());
                for (i, clause) in ast.clauses.iter().enumerate() {
                    println!("    Clause {}: {:?}", i, clause);
                }
            }
            Err(e) => {
                println!("  ❌ Error: {:?}", e);
            }
        }
    }
}















