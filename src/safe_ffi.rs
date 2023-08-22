

// pub fn safe_deref<T>(ptr: *const T) -> T {
//     unsafe{
//         std::ptr::read(ptr)
//     }
// }

// assumption at start of functions: c_safe(ptr)

// c_safe(ptr: *T) = ptr points to a valid block of memory of size `sizeof(T)` and is correctly aligned  
//                   c/cpp respects const correctness
//                   i.e., valid(ptr)


// The pointer passed to a library function array parameter does not have a value such that all address computations and object accesses are valid (7.1.4).
// 


// TODO: initialization? uninit read is undefined behavior but uninit write is fine and even common
// -- maybe uninit?
// TODO: aliasing -- we need to make sure that pointers are not aliased at start of function / don't overlap


// guaranteed of pointer: aligned, array is valid (one has to imagine the same is true for )
// guaranteed on use: non-null, allocation not freed

// ptr = valid | null | invalid
// invalid = freed | magic number | not big enough

// I'm going to assume that all args are valid | null, because I cannot think of a valid reason that
// an invalid arg would be passed in, even though it is technically defined behavior.
// I don't have any real authority for this assumption though, which makes it a little rhetorically weak 

// rust_safe(ptr: *T) = 

// dereferencable(ptr: *T) = valid(ptr, sizeof(T))

// reference: https://doc.rust-lang.org/std/primitive.pointer.html#method.as_ref
// precondition: aligned(ptr) && dereferencable(ptr) && initialized(ptr) && !exists(mut ref of ptr) 
// postcondition: exists(ref of ptr) 
pub fn safe_as_ref<'a, T>(ptr: *const T) -> &'a T {
    unsafe{
        &*ptr
    }
}

// precondition: aligned(ptr) && (dereferencable(ptr) || null(ptr)) && initialized(ptr) && !exists(mut ref of ptr)  
// postcondition: exists(ref of ptr) 
pub fn safe_try_as_ref<'a, T>(ptr: *const T) -> Option<&'a T> {
    unsafe {
        ptr.as_ref()
    }
}

// precondition: aligned(ptr) && dereferencable(ptr) && initialized(ptr) && !exists(mut ref of ptr) && !exists(ref of ptr)  
// postcondition: exists(mut ref of ptr) 
pub fn safe_as_ref_mut<'a, T>(ptr: *mut T) -> &'a mut T {
    unsafe{
        &mut *ptr
    }
}

// precondition: aligned(ptr) && (dereferencable(ptr) || null(ptr)) && initialized(ptr) && !exists(mut ref of ptr) && !exists(ref of ptr)  
// postcondition: exists(mut ref of ptr) 
pub fn safe_try_as_ref_mut<'a, T>(ptr: *mut T) -> Option<&'a mut T> {
    unsafe {
        ptr.as_mut()
    }
}

// precondition: aligned(data) && valid(data, sizeof(T) * len) && initialized(data, sizeof(T) * len) && !exists(mut ref of ptr)  
// postcondition: exists(ref of data) 
pub fn safe_slice_from_raw_parts<'a, T>(data: *const T, len: usize) -> &'a [T]
{
    unsafe {
        std::slice::from_raw_parts(data, len)
    }
}

// precondition: aligned(data) && valid(data, sizeof(T) * len) && initialized(data, sizeof(T) * len) && !exists(mut ref of ptr) && !exists(ref of ptr)  
// postcondition: exists(mut ref of data) 
pub fn safe_slice_from_raw_parts_mut<'a, T>(data: *mut T, len: usize) -> &'a mut [T]
{
    unsafe {
        std::slice::from_raw_parts_mut(data, len)
    }
}

// TODO: we just have to discharge this precondition to the C code most of the time?
// precondition: utf8(v)
// postcondition:
pub fn safe_from_utf8_unchecked(v: &[u8]) -> &str
{
    unsafe {
        std::str::from_utf8_unchecked(v)
    }
}

// reference: https://doc.rust-lang.org/stable/std/ptr/fn.copy_nonoverlapping.html
// precondition: valid(src, sizeof(T) * count) && valid(dst, sizeof(T) * count) && aligned(src) && aligned(dst) && !distinct(src,  sizeof(T) * count, dst, sizeof(T) * count)
// postcondition: 
pub fn safe_copy_nonoverlapping<T>(src: *const T, dst: *mut T, count: usize)
{
    unsafe {
        std::ptr::copy_nonoverlapping(src, dst, count)
    }
}

pub fn safe_box_from_raw<T>(raw: *mut T) -> Box<T>
{
    unsafe {
        Box::from_raw(raw)
    }
}

// reference: https://doc.rust-lang.org/stable/std/ffi/struct.CStr.html#method.from_ptr
pub fn safe_cstr_from_ptr<'a>(ptr: *const i8) -> &'a std::ffi::CStr 
{
    unsafe {
        std::ffi::CStr::from_ptr(ptr)
    }
}

// reference: https://doc.rust-lang.org/std/sync/struct.Arc.html#method.from_raw
// TODO: preconditions / postconditions?
pub fn safe_arc_from_raw<T>(ptr: *const T) -> std::sync::Arc<T>
{
    unsafe {
        std::sync::Arc::from_raw(ptr)
    }
}