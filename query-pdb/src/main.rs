use query_pdb::DebugSymbolsCollection;

fn main() {
    let x = DebugSymbolsCollection::read_from_file("./a.pdb").unwrap();
    dbg!(x);
}
