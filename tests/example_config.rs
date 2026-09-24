//! The shipped example profile must parse and produce a valid pipeline.

#[test]
fn example_config_is_valid() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/abrightd.toml");
    let config = abrightd::config::Config::load(std::path::Path::new(path)).unwrap();
    config.mapper().unwrap();
    config.controller_config().unwrap();
    assert_eq!(config.als.kind, "iio");
    assert_eq!(config.curve.lux.len(), config.curve.bri.len());
}
