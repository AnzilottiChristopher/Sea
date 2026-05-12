#include <stdlib.h>

typedef struct {
  int x;
} Point;

int main(int argc, char *argv[]) {
  Point *p = malloc(sizeof(Point));
  free(p);
  p->x = 5;
  return 0;
}
