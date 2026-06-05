use postgres::{Client, NoTls};

#[test]
fn inspect_db() {
    let mut client = Client::connect("host=192.168.0.113 port=5432 user=admin password=admin dbname=postgres", NoTls).unwrap();
    
    // Check all tables
    let rows = client.query(
        "SELECT table_schema, table_name FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog', 'information_schema')",
        &[]
    ).unwrap();
    
    println!("--- ALL TABLES IN POSTGRES ---");
    for row in rows {
        let schema: String = row.get(0);
        let table_name: String = row.get(1);
        println!("Schema: {}, Table: {}", schema, table_name);
        
        let cols = client.query(
            "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = $1 AND table_name = $2",
            &[&schema, &table_name]
        ).unwrap();
        for col in cols {
            let col_name: String = col.get(0);
            let data_type: String = col.get(1);
            println!("  - {}: {}", col_name, data_type);
        }
    }
}
