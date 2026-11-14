use pex_as::assemble;

#[test]
fn test_instructions() {
    static PAS: &str = include_str!("instructions.pas");
    assemble(PAS).unwrap();
}
