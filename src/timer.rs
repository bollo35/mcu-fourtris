pub struct Timer0<'a> {
    p: &'a tm4c123x::Peripherals,
}


impl Timer0<'_> {
    pub fn new(p: &tm4c123x::Peripherals, interval_value: u32) -> Timer0 {
        // 0. Set the Timer0 bit in the RCGCTIMER register
        p.SYSCTL.rcgctimer.modify(|r, w| unsafe { w.bits( r.bits() | 1 ) });

        // wait for the timer to be ready for access
        while p.SYSCTL.prtimer.read().bits() & 1 != 1 {}

        // 1. Ensure timer is disabled
        p.TIMER0.ctl.modify(|r, w| unsafe { w.bits( r.bits() & (!1) ) });

        // 2. Write 0 to the GPTM configuration register
        //    this gives us a 32 bit timer
        p.TIMER0.cfg.write(|w| unsafe { w.bits(0) });

        // 3. Place timer A in periodic mode
        p.TIMER0.tamr.modify(|r, w| unsafe { w.bits( (r.bits() & (!3)) | 2 ) });

        // 4. TASNAPS and TACDIR - take a snapshot and use count up mode
        p.TIMER0.tamr.modify(|r, w| unsafe { w.bits( r.bits() | 0x90 ) });

        // 5. Load start value into TnILR register
        p.TIMER0.tailr.write(|w| unsafe { w.bits(interval_value) });

        Timer0 {
            p
        }
    }

    pub fn start(&self) {
        // 7. Enable timer
        self.p.TIMER0.ctl.modify(|r, w| unsafe { w.bits( r.bits() | 1 ) });
    }

    pub fn timeout_occured(&self) -> bool {
        self.p.TIMER0.ris.read().bits() & 1 == 1
    }

    pub fn clear_interrupt(&self) {
        self.p.TIMER0.icr.write(|w| unsafe { w.bits(1) });
    }
}
