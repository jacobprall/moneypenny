const allAccounts = (state) => {
  const keys = Object.keys(state.accounts);
  const allAccts = keys.map((key) => state.accounts[key]
  );
  return allAccts;
}

export default allAccounts