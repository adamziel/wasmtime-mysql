#ifndef MYSQL_WASI_LIBC_SHIM_H
#define MYSQL_WASI_LIBC_SHIM_H

#if defined(__wasi__)

#include <errno.h>
#include <fenv.h>
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/types.h>
#include "sys/resource.h"
#include <unistd.h>

#ifndef F_RDLCK
#define F_RDLCK 0
#endif
#ifndef F_WRLCK
#define F_WRLCK 1
#endif
#ifndef F_UNLCK
#define F_UNLCK 2
#endif
#ifndef F_GETLK
#define F_GETLK 5
#endif
#ifndef F_SETLK
#define F_SETLK 6
#endif
#ifndef F_SETLKW
#define F_SETLKW 7
#endif

#ifndef HAVE_FDATASYNC
#define HAVE_FDATASYNC 1
#endif

#ifndef SIG_SETMASK
#define SIG_SETMASK 2
#endif
#ifndef SIG_BLOCK
#define SIG_BLOCK 0
#endif
#ifndef SIG_UNBLOCK
#define SIG_UNBLOCK 1
#endif
#ifndef SA_RESETHAND
#define SA_RESETHAND 0x80000000
#endif
#ifndef SA_NODEFER
#define SA_NODEFER 0x40000000
#endif
#ifndef SA_SIGINFO
#define SA_SIGINFO 4
#endif
#ifndef ILL_ILLOPC
#define ILL_ILLOPC 1
#endif
#ifndef ILL_ILLOPN
#define ILL_ILLOPN 2
#endif
#ifndef ILL_ILLADR
#define ILL_ILLADR 3
#endif
#ifndef ILL_ILLTRP
#define ILL_ILLTRP 4
#endif
#ifndef ILL_PRVOPC
#define ILL_PRVOPC 5
#endif
#ifndef ILL_PRVREG
#define ILL_PRVREG 6
#endif
#ifndef ILL_COPROC
#define ILL_COPROC 7
#endif
#ifndef ILL_BADSTK
#define ILL_BADSTK 8
#endif
#ifndef FPE_INTDIV
#define FPE_INTDIV 1
#endif
#ifndef FPE_INTOVF
#define FPE_INTOVF 2
#endif
#ifndef FPE_FLTDIV
#define FPE_FLTDIV 3
#endif
#ifndef FPE_FLTOVF
#define FPE_FLTOVF 4
#endif
#ifndef FPE_FLTUND
#define FPE_FLTUND 5
#endif
#ifndef FPE_FLTRES
#define FPE_FLTRES 6
#endif
#ifndef FPE_FLTINV
#define FPE_FLTINV 7
#endif
#ifndef FPE_FLTSUB
#define FPE_FLTSUB 8
#endif
#ifndef SEGV_MAPERR
#define SEGV_MAPERR 1
#endif
#ifndef SEGV_ACCERR
#define SEGV_ACCERR 2
#endif
#ifndef BUS_ADRALN
#define BUS_ADRALN 1
#endif
#ifndef BUS_ADRERR
#define BUS_ADRERR 2
#endif
#ifndef BUS_OBJERR
#define BUS_OBJERR 3
#endif
#ifndef TRAP_BRKPT
#define TRAP_BRKPT 1
#endif
#ifndef TRAP_TRACE
#define TRAP_TRACE 2
#endif

typedef struct siginfo_t {
  int si_signo;
  int si_errno;
  int si_code;
  pid_t si_pid;
  uid_t si_uid;
  void *si_addr;
} siginfo_t;

struct sigaction {
  union {
    void (*sa_handler)(int);
    void (*sa_sigaction)(int, siginfo_t *, void *);
  } __sa_handler;
  sigset_t sa_mask;
  int sa_flags;
};

#define sa_handler __sa_handler.sa_handler
#define sa_sigaction __sa_handler.sa_sigaction

static inline int waasmtime_mysql_sigemptyset(sigset_t *set) {
  if (set != NULL) memset(set, 0, sizeof(*set));
  return 0;
}

static inline int waasmtime_mysql_sigfillset(sigset_t *set) {
  if (set != NULL) memset(set, 0xff, sizeof(*set));
  return 0;
}

static inline int waasmtime_mysql_sigaddset(sigset_t *set, int signum) {
  (void)set;
  (void)signum;
  return 0;
}

static inline int waasmtime_mysql_sigdelset(sigset_t *set, int signum) {
  (void)set;
  (void)signum;
  return 0;
}

static inline int waasmtime_mysql_sigismember(const sigset_t *set,
                                              int signum) {
  (void)set;
  (void)signum;
  return 0;
}

static inline int sigaction(int signum, const struct sigaction *act,
                            struct sigaction *oldact) {
  (void)signum;
  (void)act;
  if (oldact != NULL) memset(oldact, 0, sizeof(*oldact));
  return 0;
}

static inline int waasmtime_mysql_sigprocmask(int how, const sigset_t *set,
                                              sigset_t *oldset) {
  (void)how;
  (void)set;
  if (oldset != NULL) memset(oldset, 0, sizeof(*oldset));
  return 0;
}

static inline int waasmtime_mysql_pthread_sigmask(int how,
                                                  const sigset_t *set,
                                                  sigset_t *oldset) {
  return waasmtime_mysql_sigprocmask(how, set, oldset);
}

static inline int waasmtime_mysql_sigwait(const sigset_t *set, int *signum) {
  (void)set;
  if (signum != NULL) *signum = 0;
  return ENOSYS;
}

static inline int waasmtime_mysql_sigwaitinfo(const sigset_t *set,
                                              siginfo_t *info) {
  (void)set;
  if (info != NULL) memset(info, 0, sizeof(*info));
  errno = ENOSYS;
  return -1;
}

#define sigemptyset waasmtime_mysql_sigemptyset
#define sigfillset waasmtime_mysql_sigfillset
#define sigaddset waasmtime_mysql_sigaddset
#define sigdelset waasmtime_mysql_sigdelset
#define sigismember waasmtime_mysql_sigismember
#define sigprocmask waasmtime_mysql_sigprocmask
#define pthread_sigmask waasmtime_mysql_pthread_sigmask
#define sigwait waasmtime_mysql_sigwait
#define sigwaitinfo waasmtime_mysql_sigwaitinfo

static inline int fedisableexcept(int excepts) {
  (void)excepts;
  return 0;
}

typedef int (*waasmtime_mysql_qsort_r_comparator)(const void *, const void *,
                                                  void *);

static waasmtime_mysql_qsort_r_comparator
    waasmtime_mysql_qsort_r_comparator_fn = NULL;
static void *waasmtime_mysql_qsort_r_comparator_arg = NULL;

static int waasmtime_mysql_qsort_r_adapter(const void *left,
                                           const void *right) {
  return waasmtime_mysql_qsort_r_comparator_fn(
      left, right, waasmtime_mysql_qsort_r_comparator_arg);
}

static inline void qsort_r(void *base, size_t nmemb, size_t size,
                           waasmtime_mysql_qsort_r_comparator compar,
                           void *arg) {
  waasmtime_mysql_qsort_r_comparator_fn = compar;
  waasmtime_mysql_qsort_r_comparator_arg = arg;
  qsort(base, nmemb, size, waasmtime_mysql_qsort_r_adapter);
  waasmtime_mysql_qsort_r_comparator_fn = NULL;
  waasmtime_mysql_qsort_r_comparator_arg = NULL;
}

static inline off_t tell(int fd) { return lseek(fd, 0, SEEK_CUR); }

static inline int waasmtime_mysql_dup(int oldfd) {
  (void)oldfd;
  errno = ENOSYS;
  return -1;
}

static inline int waasmtime_mysql_dup2(int oldfd, int newfd) {
  if (oldfd == newfd) return newfd;
  (void)oldfd;
  (void)newfd;
  errno = ENOSYS;
  return -1;
}

#define dup waasmtime_mysql_dup
#define dup2 waasmtime_mysql_dup2

static inline int waasmtime_mysql_pthread_kill(pthread_t thread, int signum) {
  (void)thread;
  (void)signum;
  return ENOSYS;
}

#define pthread_kill waasmtime_mysql_pthread_kill

static inline int waasmtime_mysql_pthread_attr_setscope(
    pthread_attr_t *attr, int scope) {
  (void)attr;
  (void)scope;
  return 0;
}

static inline int waasmtime_mysql_pthread_setname_np(pthread_t thread,
                                                     const char *name) {
  (void)thread;
  (void)name;
  return 0;
}

#define pthread_attr_setscope waasmtime_mysql_pthread_attr_setscope
#define pthread_setname_np waasmtime_mysql_pthread_setname_np

static inline int waasmtime_mysql_msync(void *addr, size_t len, int flags) {
  (void)addr;
  (void)len;
  (void)flags;
  return 0;
}

static inline int waasmtime_mysql_madvise(void *addr, size_t len, int advice) {
  (void)addr;
  (void)len;
  (void)advice;
  return 0;
}

#define msync waasmtime_mysql_msync
#define madvise waasmtime_mysql_madvise

static inline uid_t waasmtime_mysql_getuid(void) { return 1; }
static inline uid_t waasmtime_mysql_geteuid(void) { return 1; }
static inline gid_t waasmtime_mysql_getgid(void) { return 1; }
static inline gid_t waasmtime_mysql_getegid(void) { return 1; }

static inline int waasmtime_mysql_initgroups(const char *user, gid_t group) {
  (void)user;
  (void)group;
  errno = ENOSYS;
  return -1;
}

static inline int waasmtime_mysql_setgid(gid_t group) {
  (void)group;
  errno = ENOSYS;
  return -1;
}

static inline int waasmtime_mysql_setuid(uid_t user) {
  (void)user;
  errno = ENOSYS;
  return -1;
}

static inline int waasmtime_mysql_setregid(gid_t real_group,
                                           gid_t effective_group) {
  (void)real_group;
  (void)effective_group;
  errno = ENOSYS;
  return -1;
}

static inline int waasmtime_mysql_setreuid(uid_t real_user,
                                           uid_t effective_user) {
  (void)real_user;
  (void)effective_user;
  errno = ENOSYS;
  return -1;
}

static inline int waasmtime_mysql_chroot(const char *path) {
  (void)path;
  errno = ENOSYS;
  return -1;
}

#define getuid waasmtime_mysql_getuid
#define geteuid waasmtime_mysql_geteuid
#define getgid waasmtime_mysql_getgid
#define getegid waasmtime_mysql_getegid
#define initgroups waasmtime_mysql_initgroups
#define setgid waasmtime_mysql_setgid
#define setuid waasmtime_mysql_setuid
#define setregid waasmtime_mysql_setregid
#define setreuid waasmtime_mysql_setreuid
#define chroot waasmtime_mysql_chroot

static char waasmtime_mysql_tzname_utc[] = "UTC";
static char *tzname[2] = {waasmtime_mysql_tzname_utc,
                          waasmtime_mysql_tzname_utc};

static inline void tzset(void) {}

static inline mode_t waasmtime_mysql_umask(mode_t mask) {
  (void)mask;
  return 0;
}

#define umask waasmtime_mysql_umask

static inline void *waasmtime_mysql_memalign(size_t alignment, size_t size) {
  void *ptr = NULL;
  if (posix_memalign(&ptr, alignment, size) != 0) return NULL;
  return ptr;
}

#define memalign waasmtime_mysql_memalign

static inline int waasmtime_mysql_pthread_cancel(pthread_t thread) {
  (void)thread;
  return ENOSYS;
}

#if defined(__GNUC__)
#define MYSQL_WASI_NORETURN __attribute__((__noreturn__))
#else
#define MYSQL_WASI_NORETURN
#endif

static inline MYSQL_WASI_NORETURN void waasmtime_mysql_pthread_exit(
    void *value_ptr) {
  (void)value_ptr;
  abort();
}

#undef MYSQL_WASI_NORETURN

#define pthread_cancel waasmtime_mysql_pthread_cancel
#define pthread_exit waasmtime_mysql_pthread_exit

static inline int waasmtime_mysql_chown(const char *path, uid_t owner,
                                        gid_t group) {
  (void)path;
  (void)owner;
  (void)group;
  return 0;
}

#define chown waasmtime_mysql_chown

static inline int waasmtime_mysql_mkstemp(char *pattern) {
  static const char alphabet[] = "abcdefghijklmnopqrstuvwxyz0123456789";
  size_t len = strlen(pattern);
  if (len < 6 || strcmp(pattern + len - 6, "XXXXXX") != 0) {
    errno = EINVAL;
    return -1;
  }

  for (unsigned attempt = 0; attempt < 36 * 36 * 36; ++attempt) {
    unsigned value = attempt;
    for (size_t i = 0; i < 6; ++i) {
      pattern[len - 1 - i] = alphabet[value % 36];
      value /= 36;
    }

    int fd = open(pattern, O_CREAT | O_EXCL | O_RDWR, 0600);
    if (fd >= 0) return fd;
    if (errno != EEXIST) return -1;
  }

  errno = EEXIST;
  return -1;
}

#define mkstemp waasmtime_mysql_mkstemp

#endif /* __wasi__ */

#endif /* MYSQL_WASI_LIBC_SHIM_H */
