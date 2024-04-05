## Scheduling algorithm

1. Begin with the active periods, the parts of each day where you are willing to work, and divide them into cycles. Start at the present, and continue until the last task's due date.  
   ![Ten squares, drawn side-to-side, with dotted edges.](art/schedule/01-slices.excalidraw.png)
2. Next, a set of tasks; each task has an estimated time-to-completion, a start and due date, and a priority.
   ![Below the squares, three color-coded tasks have appeared. Task A, with priority 9, can be worked on at any time, and requires seven units of time. Task B, with priority 8, can be worked on in slices 5 and 6, and requires one unit of time. Task C, with priority 7, can be worked on in slices 6-10, and requires three units of time.](art/schedule/02-tasks.excalidraw.png)
3. In ascending order of working-period-length, each task claims enough slots from the start of its working period to satisfy itself, if it can.
   ![Task B claims slot 5 and is satisfied. Task C claims slots 6-8 and is satisfied. Task A claims all of the remaining slots and still wants one more.](art/schedule/03-claim.excalidraw.png)
4. In ascending order of priority, each dissatisfied task tries to take slots in its working period, starting with the lowest-priority task. Repeat until none of the dissatisfied tasks can capture any slots.  
   (The algorithm for this is horribly slow, but this will only ever happen if you procrastinate long enough that you have to start triaging tasks.)  
   ![Step 1; A9 is not ok, B8 is OK, C7 is OK. Step 2; A9 takes 6, C7 is no longer OK. Step 3; No slots P < 7 in C7's working range, C7 fails to schedule](art/schedule/04-triage.excalidraw.png)
   
## Shuffle

Tasks are randomized according to a modified Fisher-Yates shuffle, which runs for each slot from left to right.

1. Search for slots that would be legal in our position, starting at (and including) our position, ending at the end of our working period
   1. Empty slots are legal in any position
2. Choose a slot randomly from that list
3. Swap places with it, if we didn't pick ourselves.
