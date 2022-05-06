fpe()
{
	if [ -z "$FP_NOTEBOOK_ID" ];
		then echo "Must set FP_NOTEBOOK_ID to indicate which notebook the output should be appended to";
	fi;

	# make a temp file to capture the command output
	tmp=`mktemp`
	# we use eval instead of backticks so that it runs in the same process
	eval "$@" >>$tmp
	cat $tmp

	# prepend the prompt and command to the output
	echo -e "$(pwd) ❯ $@\n\n$(cat $tmp)" > $tmp

	fp notebook append-cell $FP_NOTEBOOK_ID --code="$(cat $tmp)"

	rm $tmp
}

m()
{
	if [ -z "$FP_NOTEBOOK_ID" ];
		then echo "Must set FP_NOTEBOOK_ID to indicate which notebook the output should be appended to";
	fi;
	if [ -z "$FP_USER_NAME" ];
		then export FP_USER_NAME=$(fp user profile --output=json | jq -r '.name');
	fi;

	message="$@"
	fp notebook append-cell $FP_NOTEBOOK_ID --text="💬 $(date -u) @$FP_USER_NAME:  $message"
}

