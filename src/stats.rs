use apca::data::v2::bars;
use ta::{
    indicators::{BollingerBands, BollingerBandsOutput, RelativeStrengthIndex},
    Next,
};

pub(crate) trait Statistics {
    fn bollinger(&self) -> Option<BollingerBandsOutput>;
    fn rsi(&self) -> Option<f64>;
}

impl Statistics for Vec<bars::Bar> {
    fn bollinger(&self) -> Option<BollingerBandsOutput> {
        self.split_last().map(|(last, first)| {
            let mut bb = BollingerBands::new(self.len(), 2.0).unwrap();

            for bar in first {
                bb.next(bar.close.to_f64().unwrap_or(f64::NAN));
            }

            bb.next(last.close.to_f64().unwrap_or(f64::NAN))
        })
    }

    fn rsi(&self) -> Option<f64> {
        self.split_last().map(|(last, first)| {
            let mut bb = RelativeStrengthIndex::new(self.len()).unwrap();

            for bar in first {
                bb.next(bar.close.to_f64().unwrap_or(f64::NAN));
            }

            bb.next(last.close.to_f64().unwrap_or(f64::NAN))
        })
    }
}
