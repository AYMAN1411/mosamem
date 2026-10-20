#!/bin/zsh
# front.sh on   : remembers the frontmost app and brings Mosamem to the front
# front.sh off  : gives focus back to the remembered app
state=/tmp/mosamem-prev-front
if [[ $1 == on ]]; then
  osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' > $state 2>/dev/null
  osascript -e 'tell application "System Events" to set frontmost of (first process whose name is "mosamem") to true' >/dev/null
else
  prev=$(cat $state 2>/dev/null)
  [[ -n $prev && $prev != mosamem ]] && osascript -e "tell application \"System Events\" to set frontmost of process \"$prev\" to true" >/dev/null
fi
exit 0
