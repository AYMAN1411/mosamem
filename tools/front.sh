#!/bin/zsh
# front.sh on   : remembers the frontmost app and brings Mosamem to the front
# front.sh off  : gives focus back to the remembered app
state=/tmp/mosamem-prev-front
if [[ $1 == on ]]; then
  osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' > $state 2>/dev/null
  # the dev or harness build (an installed Mosamem has the same process name, so go by its id)
  pid=$(pgrep -f 'target/(debug|release|harness)/.*mosamem$' | head -1)
  if [[ -n $pid ]]; then
    osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $pid) to true" >/dev/null
  else
    osascript -e 'tell application "System Events" to set frontmost of (first process whose name is "mosamem") to true' >/dev/null
  fi
else
  prev=$(cat $state 2>/dev/null)
  [[ -n $prev && $prev != mosamem ]] && osascript -e "tell application \"System Events\" to set frontmost of process \"$prev\" to true" >/dev/null
fi
exit 0
