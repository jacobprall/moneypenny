import React, { useEffect, useState } from 'react'
import AccountCategory from './account_category'
import NetWorth from './net_worth'


export default function accounts_index({accounts, getAccounts}) {
 
  // const [accounts, setAccounts] = useState([])
  useEffect(() => {
    getAccounts()
  }, [])


  
  console.log(accounts)
  
  const cash = accounts.filter((account) => (
    account.account_category === 'Cash'
  ));

  const creditCards = accounts.filter((account) => (
    account.account_category === 'Credit Cards'
  ));

  const loans = accounts.filter((account) => (
    account.account_category === 'Loans'
  ));

  const investments = accounts.filter((account) => (
    account.account_category === 'Investments'
  ));

  const property = accounts.filter((account) => (
    account.account_category === 'Property'
  ));

  return (
    <div className='accounts-index-container'>
      <AccountCategory accounts={cash} category="Cash" logo={window.money}/>
      <AccountCategory accounts={creditCards} category="Credit Cards" logo={window.card}/>
      <AccountCategory accounts={loans} category="Loans" logo={window.cap}/>
      <AccountCategory accounts={investments} category="Investments" logo={window.chart}/>
      <AccountCategory accounts={property} category="Property" logo={window.house} />
      <NetWorth accounts={accounts} />
    </div>
  )
}
