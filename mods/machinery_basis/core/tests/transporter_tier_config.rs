//! `TransporterTier.getSpeed` e `getPullAmount` leem o arquivo `tiers.toml` (`TierConfig.addLogisticalTransportersCategory`): a cola do Mekanism põe os valores do arquivo quando ele
//! carrega, e o transportador (o `Transporter` e o dono) passa a usá-los. Os padrões valem antes disso.
use machinery_basis_core::transporter::node::TransporterTier;

#[test]
fn the_tier_values_are_the_defaults_until_the_file_sets_them_and_then_the_file_wins() {
    let speeds = TransporterTier::VALUES.map(TransporterTier::speed);
    let pulls = TransporterTier::VALUES.map(TransporterTier::pull_amount);
    assert_eq!(speeds, [5, 10, 20, 50]);
    assert_eq!(pulls, [1, 16, 32, 64]);
    TransporterTier::configure([6, 11, 21, 51], [2, 17, 33, 65]);
    assert_eq!(
        TransporterTier::VALUES.map(TransporterTier::speed),
        [6, 11, 21, 51]
    );
    assert_eq!(
        TransporterTier::VALUES.map(TransporterTier::pull_amount),
        [2, 17, 33, 65]
    );
    TransporterTier::configure(speeds, pulls);
    assert_eq!(TransporterTier::Ultimate.speed(), 50);
}
