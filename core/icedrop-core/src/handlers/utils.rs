macro_rules! def_frame_selector {
    ($name:ident, $($frame_ty:ident),+) => {
        #[derive(Debug)]
        pub enum $name {
            $(
                $frame_ty($frame_ty),
            )+
        }

        impl Frame for $name {
            fn frame_type(&self) -> u16 {
                match self {
                    $(
                        $name::$frame_ty(frame) => frame.frame_type(),
                    )+
                }
            }

            fn try_parse(frame_type: u16, buf: Vec<u8>) -> FrameParsingResult<Self> {
                $(
                    let result = $frame_ty::try_parse(frame_type, buf);
                    if let FrameParsingResult::Ok(frame) = result {
                        return FrameParsingResult::Ok($name::$frame_ty(frame));
                    } else if let FrameParsingResult::Err(err) = result {
                        return FrameParsingResult::Err(err);
                    }
                    let buf = result.unwrap_buf();
                )+

                return FrameParsingResult::Skip(buf);
            }

            fn to_bytes(self) -> Vec<u8> {
                match self {
                    $(
                        $name::$frame_ty(frame) => frame.to_bytes(),
                    )+
                }
            }
        }
    };
}

pub(super) use def_frame_selector;
