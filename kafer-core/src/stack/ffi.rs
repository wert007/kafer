#[repr(C)]
#[derive(Default, Clone)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct RUNTIME_FUNCTION {
    pub BeginAddress: u32,
    pub EndAddress: u32,
    pub UnwindInfo: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct UNWIND_INFO {
    pub version_flags: u8,
    pub size_of_prolog: u8,
    pub count_of_codes: u8,
    pub frame_register_offset: u8,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct UNWIND_CODE {
    pub code_offset: u8,
    pub unwind_op_info: u8,
}

// const UWOP_PUSH_NONVOL: u8 = 0; /* info == register number */
// const UWOP_ALLOC_LARGE: u8 = 1; /* no info, alloc size in next 2 slots */
// const UWOP_ALLOC_SMALL: u8 = 2; /* info == size of allocation / 8 - 1 */
// const UWOP_SET_FPREG: u8 = 3; /* no info, FP = RSP + UNWIND_INFO.FPRegOffset*16 */
// const UWOP_SAVE_NONVOL: u8 = 4; /* info == register number, offset in next slot */
// const UWOP_SAVE_NONVOL_FAR: u8 = 5; /* info == register number, offset in next 2 slots */
// const UWOP_SAVE_XMM128: u8 = 8; /* info == XMM reg number, offset in next slot */
// const UWOP_SAVE_XMM128_FAR: u8 = 9; /* info == XMM reg number, offset in next 2 slots */
// const UWOP_PUSH_MACHFRAME: u8 = 10; /* info == 0: no error-code, 1: error-code */
// const UNW_FLAG_NHANDLER: u8 = 0x0;
// const UNW_FLAG_EHANDLER: u8 = 0x1;
// const UNW_FLAG_UHANDLER: u8 = 0x2;
// const UNW_FLAG_CHAININFO: u8 = 0x4;
