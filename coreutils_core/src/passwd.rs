//! Module to deal more easily with UNIX passwd.

use std::{
    convert::TryFrom,
    error::Error as StdError,
    ffi::CStr,
    fmt::{self, Display},
    mem::MaybeUninit,
    os::raw::c_char,
    ptr,
    result::Result as StdResult,
};

#[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
use crate::types::Fields;
#[cfg(not(any(
    target_os = "linux",
    target_os = "haiku",
    target_os = "fuchsia",
    target_os = "solaris"
)))]
use crate::types::Time;
use crate::{
    group::{Error as GrError, Groups},
    types::{Gid, Uid},
};

use self::Error::*;

use libc::{geteuid, getpwnam_r, getpwuid_r, getuid, passwd};

use bstr::{BStr, BString, ByteSlice};

pub type Result<T> = StdResult<T, Error>;

/// This struct holds information about a passwd of UNIX/UNIX-like systems.
///
/// Contains `sys/types.h` `passwd` struct attributes as Rust more powefull types.
#[derive(Debug)]
pub enum Error {
    /// Happens when `getpwgid_r` or `getpwnam_r` fails.
    ///
    /// It holds the the function that was used and a error code of the function return.
    GetPasswdFailed(String, i32),
    /// Happens when the pointer to the `.pw_name` is NULL.
    NameCheckFailed,
    /// Happens when the pointer to the `.pw_passwd` is NULL.
    PasswdCheckFailed,
    /// Happens when the pointer to the `.pw_gecos` is NULL.
    GecosCheckFailed,
    /// Happens when the pointer to the `.pw_dir` is NULL.
    DirCheckFailed,
    /// Happens when the pointer to the `.pw_shell` is NULL.
    ShellCheckFailed,
    /// Happens when the pointer to the `.pw_class` is NULL.
    ClassCheckFailed,
    /// Happens when the pointer to the `.pw_age` is NULL.
    AgeCheckFailed,
    /// Happens when the pointer to the `.pw_comment` is NULL.
    CommentCheckFailed,
    /// Happens when the passwd is not found.
    PasswdNotFound,
    /// Happens when something happens when finding what `Group` a `Passwd` belongs
    Group(Box<GrError>),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GetPasswdFailed(fn_name, err_code) => write!(
                f,
                "Failed to get passwd with the following error code: {}. For more info search for \
                 the {} manual",
                err_code, fn_name
            ),
            NameCheckFailed => write!(f, "Passwd name check failed, `.pw_name` field is null"),
            PasswdCheckFailed => write!(f, "Passwd passwd check failed, `.pw_passwd` is null"),
            GecosCheckFailed => write!(f, "Passwd gecos check failed, `.pw_gecos` is null"),
            DirCheckFailed => write!(f, "Passwd dir check failed, `.pw_dir` is null"),
            ShellCheckFailed => write!(f, "Passwd shell check failed, `.pw_shell` is null"),
            ClassCheckFailed => write!(f, "Passwd class check failed, `.pw_class` is null"),
            AgeCheckFailed => write!(f, "Passwd class check failed, `.pw_age` is null"),
            CommentCheckFailed => write!(f, "Passwd class check failed, `.pw_comment` is null"),
            PasswdNotFound => write!(f, "Passwd was not found in the system"),
            Group(err) => {
                write!(f, "The following error hapenned trying to get all `Groups`: {}", err)
            },
        }
    }
}

impl StdError for Error {
    #[inline]
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Group(err) => Some(err),
            _ => None,
        }
    }
}

impl From<GrError> for Error {
    #[inline]
    fn from(err: GrError) -> Error { Group(Box::new(err)) }
}

/// This struct holds the information of a user in UNIX/UNIX-like systems.
///
/// Contains `sys/types.h` `passwd` struct attributes as Rust more common types.
// It also contains a pointer to the libc::passwd type for more complex manipulations.
#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct Passwd {
    /// User login name.
    name:     BString,
    /// User encrypted password.
    passwd:   BString,
    /// User ID.
    user_id:  Uid,
    /// User Group ID.
    group_id: Gid,
    /// User full name.
    gecos:    BString,
    /// User directory.
    dir:      BString,
    /// User login shell
    shell:    BString,
    /// Password change time
    #[cfg(not(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    )))]
    change:   Time,
    /// User access class
    #[cfg(not(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    )))]
    class:    BString,
    /// Account expiration
    #[cfg(not(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    )))]
    expire:   Time,
    /// Fields filled in
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    fields:   Fields,
    #[cfg(target_os = "solaris")]
    age:      BString,
    #[cfg(target_os = "solaris")]
    comment:  BString,
}

impl Passwd {
    /// Create a new `Passwd` getting the current process user passwd as default using the
    /// effective user id.
    ///
    /// It may fail, so return a `Result`, either the `Passwd` struct wrapped in a `Ok`,
    /// or a `Error` wrapped in a `Err`.
    pub fn effective() -> Result<Self> {
        let mut buff = [0; 16384]; // Got this size from manual page about getpwuid_r
        let mut pw = MaybeUninit::zeroed();
        let mut pw_ptr = ptr::null_mut();

        let res = unsafe {
            getpwuid_r(geteuid(), pw.as_mut_ptr(), &mut buff[0], buff.len(), &mut pw_ptr)
        };

        if pw_ptr.is_null() {
            if res == 0 {
                return Err(PasswdNotFound);
            } else {
                return Err(GetPasswdFailed(String::from("getpwnam_r"), res));
            }
        }

        // Now that pw is initialized we get it
        let pw = unsafe { pw.assume_init() };

        Ok(Passwd::try_from(pw)?)
    }

    /// Create a new `Passwd` getting the current process user passwd as default using the
    /// real user id.
    ///
    /// It may fail, so return a `Result`, either the `Passwd` struct wrapped in a `Ok`,
    /// or a `Error` wrapped in a `Err`.
    pub fn real() -> Result<Self> {
        let mut buff = [0; 16384]; // Got this size from manual page about getpwuid_r
        let mut pw = MaybeUninit::zeroed();
        let mut pw_ptr = ptr::null_mut();

        let res =
            unsafe { getpwuid_r(getuid(), pw.as_mut_ptr(), &mut buff[0], buff.len(), &mut pw_ptr) };

        if pw_ptr.is_null() {
            if res == 0 {
                return Err(PasswdNotFound);
            } else {
                return Err(GetPasswdFailed(String::from("getpwnam_r"), res));
            }
        }

        // Now that pw is initialized we get it
        let pw = unsafe { pw.assume_init() };

        Ok(Passwd::try_from(pw)?)
    }

    /// Create a new `Passwd` using a `id` to get all attributes.
    ///
    /// It may fail, so return a `Result`, either the `Passwd` struct wrapped in a `Ok`,
    /// or a `Error` wrapped in a `Err`.
    pub fn from_uid(id: Uid) -> Result<Self> {
        let mut buff = [0; 16384]; // Got this size from manual page about getpwuid_r
        let mut pw = MaybeUninit::zeroed();
        let mut pw_ptr = ptr::null_mut();

        let res = unsafe { getpwuid_r(id, pw.as_mut_ptr(), &mut buff[0], buff.len(), &mut pw_ptr) };

        if pw_ptr.is_null() {
            if res == 0 {
                return Err(PasswdNotFound);
            } else {
                return Err(GetPasswdFailed(String::from("getpwnam_r"), res));
            }
        }

        // Now that pw is initialized we get it
        let pw = unsafe { pw.assume_init() };

        Ok(Passwd::try_from(pw)?)
    }

    /// Create a new `Passwd` using a `name` to get all attributes.
    ///
    /// It may fail, so return a `Result`, either the `Passwd` struct wrapped in a `Ok`,
    /// or a `Error` wrapped in a `Err`.
    pub fn from_name(name: &str) -> Result<Self> {
        let mut pw = MaybeUninit::zeroed();
        let mut pw_ptr = ptr::null_mut();
        let mut buff = [0; 16384]; // Got this size from manual page about getpwuid_r

        let name_null = {
            let mut n = BString::from(name);
            n.push(b'\0');
            n
        };

        let res = unsafe {
            getpwnam_r(
                name_null.as_ptr() as *const c_char,
                pw.as_mut_ptr(),
                &mut buff[0],
                buff.len(),
                &mut pw_ptr,
            )
        };

        if pw_ptr.is_null() {
            if res == 0 {
                return Err(PasswdNotFound);
            } else {
                return Err(GetPasswdFailed(String::from("getpwnam_r"), res));
            }
        }

        // Now that pw is initialized we get it
        let pw = unsafe { pw.assume_init() };

        Ok(Passwd::try_from(pw)?)
    }

    /// Get `Passwd` login name.
    #[inline]
    pub fn name(&self) -> &BStr { self.name.as_bstr() }

    /// Get `Passwd` encrypted password.
    #[inline]
    pub fn passwd(&self) -> &BStr { self.passwd.as_bstr() }

    /// Get `Passwd` user ID.
    #[inline]
    pub fn uid(&self) -> Uid { self.user_id }

    /// Get `Passwd` group ID.
    #[inline]
    pub fn gid(&self) -> Gid { self.group_id }

    /// Get `Passwd` full name.
    #[inline]
    pub fn gecos(&self) -> &BStr { self.gecos.as_bstr() }

    /// Get `Passwd` dir.
    #[inline]
    pub fn dir(&self) -> &BStr { self.dir.as_bstr() }

    /// Get `Passwd` shell.
    #[inline]
    pub fn shell(&self) -> &BStr { self.shell.as_bstr() }

    /// Get `Passwd` access class.
    #[inline]
    #[cfg(not(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    )))]
    pub fn class(&self) -> &BStr { &self.class.as_bstr() }

    /// Get `Passwd` password change time
    #[inline]
    #[cfg(not(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    )))]
    pub fn password_change(&self) -> Time { self.change }

    /// Get `Passwd` expiration time
    #[inline]
    #[cfg(not(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    )))]
    pub fn expire(&self) -> Time { self.expire }

    /// Get `Passwd` fields filled in
    #[inline]
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    pub fn fields(&self) -> Fields { self.fields }

    /// Get the groups that `Passwd` belongs to.
    pub fn belongs_to(&self) -> Result<Groups> {
        let name = {
            let mut n = self.name.to_string();
            n.push('\0');
            n
        };
        let gr = Groups::from_username(&name)?;
        Ok(gr)
    }
}

impl TryFrom<passwd> for Passwd {
    type Error = Error;

    fn try_from(pw: passwd) -> StdResult<Self, Self::Error> {
        let name = if pw.pw_name.is_null() {
            return Err(NameCheckFailed);
        } else {
            let name_cstr = unsafe { CStr::from_ptr(pw.pw_name) };
            BString::from(name_cstr.to_bytes())
        };

        let passwd = if pw.pw_passwd.is_null() {
            return Err(PasswdCheckFailed);
        } else {
            let passwd_cstr = unsafe { CStr::from_ptr(pw.pw_passwd) };
            BString::from(passwd_cstr.to_bytes())
        };

        let user_id = pw.pw_uid;

        let group_id = pw.pw_gid;

        let gecos = if pw.pw_gecos.is_null() {
            return Err(GecosCheckFailed);
        } else {
            let gecos_cstr = unsafe { CStr::from_ptr(pw.pw_gecos) };
            BString::from(gecos_cstr.to_bytes())
        };

        let dir = if pw.pw_dir.is_null() {
            return Err(DirCheckFailed);
        } else {
            let dir_cstr = unsafe { CStr::from_ptr(pw.pw_dir) };
            BString::from(dir_cstr.to_bytes())
        };

        let shell = if pw.pw_shell.is_null() {
            return Err(ShellCheckFailed);
        } else {
            let shell_cstr = unsafe { CStr::from_ptr(pw.pw_shell) };
            BString::from(shell_cstr.to_bytes())
        };

        #[cfg(not(any(
            target_os = "linux",
            target_os = "haiku",
            target_os = "fuchsia",
            target_os = "solaris"
        )))]
        let change = pw.pw_change;

        #[cfg(not(any(
            target_os = "linux",
            target_os = "haiku",
            target_os = "fuchsia",
            target_os = "solaris"
        )))]
        let expire = pw.pw_expire;

        #[cfg(not(any(
            target_os = "linux",
            target_os = "haiku",
            target_os = "fuchsia",
            target_os = "solaris"
        )))]
        let class = if pw.pw_class.is_null() {
            return Err(ClassCheckFailed);
        } else {
            let class_cstr = unsafe { CStr::from_ptr(pw.pw_class) };
            BString::from(class_cstr.to_bytes())
        };

        #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
        let fields = pw.pw_fields;

        #[cfg(target_os = "solaris")]
        let age = if pw.pw_age.is_null() {
            return Err(AgeCheckFailed);
        } else {
            let class_cstr = unsafe { CStr::from_ptr(pw.pw_age) };
            BString::from(class_cstr.to_bytes())
        };

        #[cfg(target_os = "solaris")]
        let comment = if pw.pw_comment.is_null() {
            return Err(CommentCheckFailed);
        } else {
            let class_cstr = unsafe { CStr::from_ptr(pw.pw_comment) };
            BString::from(class_cstr.to_bytes())
        };

        Ok(Passwd {
            name,
            passwd,
            user_id,
            group_id,
            gecos,
            dir,
            shell,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "haiku",
                target_os = "fuchsia",
                target_os = "solaris"
            )))]
            change,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "haiku",
                target_os = "fuchsia",
                target_os = "solaris"
            )))]
            class,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "haiku",
                target_os = "fuchsia",
                target_os = "solaris"
            )))]
            expire,
            #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
            fields,
            #[cfg(target_os = "solaris")]
            age,
            #[cfg(target_os = "solaris")]
            comment,
        })
    }
}

impl Display for Passwd {
    #[cfg(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    ))]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}:{}:{}:{}",
            self.name, self.passwd, self.user_id, self.group_id, self.gecos, self.dir, self.shell
        )
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "haiku",
        target_os = "fuchsia",
        target_os = "solaris"
    )))]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            self.name,
            self.passwd,
            self.user_id,
            self.group_id,
            self.class,
            self.change,
            self.expire,
            self.gecos,
            self.dir,
            self.shell
        )
    }
}

// Extra trait impl
impl From<Passwd> for passwd {
    fn from(mut pw: Passwd) -> Self {
        passwd {
            pw_name: pw.name.as_mut_ptr() as *mut c_char,
            pw_passwd: pw.passwd.as_mut_ptr() as *mut c_char,
            pw_uid: pw.user_id,
            pw_gid: pw.group_id,
            pw_gecos: pw.gecos.as_mut_ptr() as *mut c_char,
            pw_dir: pw.dir.as_mut_ptr() as *mut c_char,
            pw_shell: pw.shell.as_mut_ptr() as *mut c_char,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "haiku",
                target_os = "fuchsia",
                target_os = "solaris"
            )))]
            pw_change: pw.change,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "haiku",
                target_os = "fuchsia",
                target_os = "solaris"
            )))]
            pw_class: pw.class.as_mut_ptr() as *mut c_char,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "haiku",
                target_os = "fuchsia",
                target_os = "solaris"
            )))]
            pw_expire: pw.expire,
            #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
            pw_fields: pw.fields,
            #[cfg(target_os = "solaris")]
            pw_age: pw.age.as_mut_ptr() as *mut c_char,
            #[cfg(target_os = "solaris")]
            pw_comment: pw.comment.as_mut_ptr() as *mut c_char,
        }
    }
}
