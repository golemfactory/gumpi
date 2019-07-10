#include <omp.h>
#include <stdio.h>

int main() {
    int num_threads = omp_get_max_threads();
    printf("%d\n", num_threads);
}