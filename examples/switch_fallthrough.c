#include <stdlib.h>
int main(int argc, char *argv[]) {
  int *p = malloc(sizeof(int));
  switch (argc) {
  case 1:
    free(p);
  case 2:
    *p = 5;
    break;
  }
  return 0;
}
