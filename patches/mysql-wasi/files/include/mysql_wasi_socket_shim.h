#ifndef MYSQL_WASI_SOCKET_SHIM_H
#define MYSQL_WASI_SOCKET_SHIM_H

#if defined(__wasi__)

#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <time.h>

#ifndef SOL_SOCKET
#define SOL_SOCKET 1
#endif
#ifndef SO_REUSEADDR
#define SO_REUSEADDR 2
#endif
#ifndef SO_ERROR
#define SO_ERROR 4
#endif
#ifndef SO_KEEPALIVE
#define SO_KEEPALIVE 9
#endif
#ifndef SO_RCVTIMEO
#define SO_RCVTIMEO 20
#endif
#ifndef SO_SNDTIMEO
#define SO_SNDTIMEO 21
#endif
#ifndef IPPROTO_IP
#define IPPROTO_IP 0
#endif
#ifndef IPPROTO_TCP
#define IPPROTO_TCP 6
#endif
#ifndef IPPROTO_IPV6
#define IPPROTO_IPV6 41
#endif
#ifndef IP_TOS
#define IP_TOS 1
#endif
#ifndef TCP_NODELAY
#define TCP_NODELAY 1
#endif
#ifndef IPV6_V6ONLY
#define IPV6_V6ONLY 26
#endif
#ifndef MSG_PEEK
#define MSG_PEEK 0x0002
#endif

#ifdef __cplusplus
extern "C" {
#endif

int waasmtime_mysql_socket(int domain, int type, int protocol);
int waasmtime_mysql_bind(int fd, const struct sockaddr *addr, socklen_t len);
int waasmtime_mysql_listen(int fd, int backlog);
int waasmtime_mysql_accept(int fd, struct sockaddr *addr, socklen_t *addr_len);
int waasmtime_mysql_connect(int fd, const struct sockaddr *addr, socklen_t len);
int waasmtime_mysql_getsockname(int fd, struct sockaddr *addr,
                                socklen_t *addr_len);
int waasmtime_mysql_getpeername(int fd, struct sockaddr *addr,
                                socklen_t *addr_len);
int waasmtime_mysql_setsockopt(int fd, int level, int optname,
                               const void *optval, socklen_t optlen);
int waasmtime_mysql_getsockopt(int fd, int level, int optname, void *optval,
                               socklen_t *optlen);
ssize_t waasmtime_mysql_send(int fd, const void *buf, size_t len, int flags);
ssize_t waasmtime_mysql_recv(int fd, void *buf, size_t len, int flags);
ssize_t waasmtime_mysql_sendto(int fd, const void *buf, size_t len, int flags,
                               const struct sockaddr *addr,
                               socklen_t addr_len);
ssize_t waasmtime_mysql_recvfrom(int fd, void *buf, size_t len, int flags,
                                 struct sockaddr *addr, socklen_t *addr_len);
int waasmtime_mysql_shutdown(int fd, int how);
int waasmtime_mysql_close(int fd);
int waasmtime_mysql_fcntl(int fd, int cmd, ...);
int waasmtime_mysql_poll(struct pollfd *fds, nfds_t nfds, int timeout);
int waasmtime_mysql_ppoll(struct pollfd *fds, nfds_t nfds,
                          const struct timespec *timeout,
                          const void *sigmask);

#ifdef __cplusplus
}
#endif

#endif /* __wasi__ */

#endif /* MYSQL_WASI_SOCKET_SHIM_H */
