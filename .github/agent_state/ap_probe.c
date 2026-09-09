// Affinity probe: parent forks 3 kids; kid i pins to CPU i+1 and prints.
// A missing ALIVE line + hung parent => that AP never schedules.
// sched_setaffinity failure (errno) prints and exits 2 (runs on BSP then).
#define _GNU_SOURCE
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <sys/wait.h>
int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    printf("PROBE-START ncpu=?\n");
    for (int i = 1; i <= 3; i++) {
        pid_t p = fork();
        if (p < 0) { printf("FORK-FAIL %d errno=%d\n", i, errno); continue; }
        if (p == 0) {
            cpu_set_t m; CPU_ZERO(&m); CPU_SET(i, &m);
            if (sched_setaffinity(0, sizeof(m), &m) != 0) {
                printf("AP%d-AFFINITY-FAIL errno=%d\n", i, errno);
                _exit(2);
            }
            printf("AP%d-ALIVE\n", i);
            _exit(0);
        }
    }
    for (int i = 1; i <= 3; i++) { int st; wait(&st); printf("REAPED status=%d\n", st); }
    printf("PROBE-DONE\n");
    return 0;
}
