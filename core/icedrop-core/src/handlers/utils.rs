macro_rules! def_frame_selector {
    ($name:ident, $($frame_ty:ident),+) => {
        #[derive(Debug)]
        pub enum $name {
            $(
                $frame_ty($frame_ty),
            )+
        }

        #[async_trait]
        impl Frame for $name {
            fn frame_type(&self) -> u16 {
                match self {
                    $(
                        $name::$frame_ty(frame) => frame.frame_type(),
                    )+
                }
            }

            async fn parse<S>(
                frame_type: u16,
                stream: &mut S,
            ) -> Option<Result<Self, Box<dyn Error + Send>>>
            where
                S: Stream,
            {
                $(
                    if let Some(frame) = $frame_ty::parse(frame_type, stream).await {
                        return Some(Ok(Self::$frame_ty(frame.unwrap())));
                    }
                )+

                return None;
            }

            fn to_bytes(&self) -> Vec<u8> {
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
