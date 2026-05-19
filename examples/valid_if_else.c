#include <stdlib.h>
int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  if (argc > 1) {
    free(p);
  } else {
    free(p);
  }
  return 0; // fine — p is freed on both paths
}
