use std::os::raw::c_uint;

macro_rules! impl_flags_ext {
    ($num_ty: ty) => {
        impl FlagsExt for $num_ty {
            #[inline]
            fn contains(&self, rhs: Self) -> bool {
                (*self & rhs) == rhs
            }
        }
    };
}

pub trait FlagsExt {
    fn contains(&self, rhs: Self) -> bool;
}

impl_flags_ext!(usize);
impl_flags_ext!(c_uint);
