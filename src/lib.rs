#![no_std]
//!

use cortex_m;
use embedded_hal::timer::CountDown;
use embedded_time::{
    duration::{Extensions, Microseconds},
    fixed_point::FixedPoint,
};
use rp2040_hal::pio::{Instance, PIO};
use smart_leds_trait::SmartLedsWrite;

/// Instance of WS2812 LED chain.
pub struct Ws2812<P: Instance, C: CountDown> {
    pio: PIO<P>,
    cd: C,
}

impl<P: Instance, C> Ws2812<P, C>
where
    C: CountDown,
{
    /// Creates a new instance of this driver.
    pub fn new(
        pin_id: u8,
        pio: P,
        resets: &mut rp2040_hal::pac::RESETS,
        clock_freq: embedded_time::rate::Hertz,
        cd: C,
    ) -> Ws2812<P, C> {
        let program = pio_proc::pio!(
            32,
            "
            .side_set 1
            .define public T1 2
            .define public T2 5
            .define public T3 3

              set pindirs, 0  side 0 [0]
            bitloop:
            .wrap_target
              out x, 1        side 0 [T3 - 1]
              jmp !x do_zero  side 1 [T1 - 1]
              jmp bitloop     side 0 [T2 - 1]
            do_zero:
              nop             side 0 [T2 - 1]
            .wrap
            "
        );
        // prepare the PIO program
        let side_set = pio::SideSet::new(false, 1, false);
        //let mut a = pio::Assembler::<32>::new_with_side_set(side_set);

        const T1: u8 = 2; // start bit
        const T2: u8 = 5; // data bit
        const T3: u8 = 3; // stop bit
        const CYCLES_PER_BIT: u32 = (T1 + T2 + T3) as u32;
        const FREQ: u32 = 800_000;

        //let mut wrap_target = a.label();
        //let mut wrap_source = a.label();
        //let mut do_zero = a.label();
        //// sets pin as Out
        //a.set_with_side_set(pio::SetDestination::PINDIRS, 1, 0);
        //a.bind(&mut wrap_target);
        //// Do stop bit
        //a.out_with_delay_and_side_set(pio::OutDestination::X, 1, T3 - 1, 0);
        //// Do start bit
        //a.jmp_with_delay_and_side_set(pio::JmpCondition::XIsZero, &mut do_zero, T1 - 1, 1);
        //// Do data bit = 1
        //a.jmp_with_delay_and_side_set(pio::JmpCondition::Always, &mut wrap_target, T2 - 1, 1);
        //a.bind(&mut do_zero);
        //// Pseudoinstruction: NOP
        //// Do data bit = 0
        //a.mov_with_delay_and_side_set(
        //    pio::MovDestination::Y,
        //    pio::MovOperation::None,
        //    pio::MovSource::Y,
        //    T2 - 2, // 1 extra cycle in the loop
        //    0,
        //);
        //a.bind(&mut wrap_source);
        //let program = a.assemble_with_wrap(wrap_source, wrap_target);

        // setup the PIO
        let pio = rp2040_hal::pio::PIO::new(pio, resets);
        let sm = &pio.state_machines()[0];

        let div = clock_freq.integer() as f32 / (FREQ as f32 * CYCLES_PER_BIT as f32);

        rp2040_hal::pio::PIOBuilder::default()
            .with_program(&program)
            // only use TX FIFO
            .buffers(rp2040_hal::pio::Buffers::OnlyTx)
            // Pin configuration
            .set_pins(pin_id, 1)
            .side_set(side_set)
            .side_set_pin_base(pin_id)
            // OSR config
            .out_shift_direction(rp2040_hal::pio::ShiftDirection::Left)
            .autopull(true)
            .pull_threshold(24)
            .clock_divisor(div)
            .build(&pio, sm)
            .unwrap();

        Self { pio, cd }
    }
}

impl<P: Instance, C> SmartLedsWrite for Ws2812<P, C>
where
    C: embedded_hal::timer::CountDown,
    C::Time: From<Microseconds>,
{
    type Color = smart_leds_trait::RGB8;
    type Error = ();
    fn write<T, I>(&mut self, iterator: T) -> Result<(), ()>
    where
        T: Iterator<Item = I>,
        I: Into<Self::Color>,
    {
        self.cd.start(60.microseconds());
        let _ = nb::block!(self.cd.wait());

        for item in iterator {
            let color: Self::Color = item.into();
            let word =
                (u32::from(color.g) << 24) | (u32::from(color.r) << 16) | (u32::from(color.b) << 8);

            while !self.pio.state_machines()[0].write_tx(word) {
                cortex_m::asm::nop();
            }
        }
        Ok(())
    }
}
