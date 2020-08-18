const allAccounts = ({entities}) => {
  const keys = Object.keys(entities.accounts);
  const allAccts = keys.map((key) => entities.accounts[key]
  );
  return allAccts;
}

export default allAccounts