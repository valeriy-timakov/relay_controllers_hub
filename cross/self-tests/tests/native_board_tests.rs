#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;



#[defmt_test::tests]
mod tests {
    use stm32f4xx_hal::pac::Peripherals;

    use board::Board;
    use defmt::{assert_eq, unwrap};

    #[init]
    fn init() -> Board {
        let cm_periph = unwrap!(Peripherals::take());
        Board::init(cm_periph, 84_000_000)
    }

    #[test]
    fn test1(board: &mut Board) {
        //assert_eq!(EXPECTED, board.scd30.get_firmware_version().unwrap())
    }
}
