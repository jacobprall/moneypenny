import React from 'react'
import AccountCategory from './account_category'
import NetWorth from './net_worth'

export default function accounts_index({accounts}) {
  const cash = accounts.filter((account) => (
    account.category === 'Cash'
  ));

  const creditCards = accounts.filter((account) => (
    account.category === 'Credit Cards'
  ));

  const loans = accounts.filter((account) => (
    account.category === 'Loans'
  ));

  const investments = accounts.filter((account) => (
    account.category === 'Investments'
  ));

  const property = accounts.filter((account) => (
    account.category === 'Property'
  ));

  return (
    <div className='accounts-index-container'>
      <AccountCategory accounts={cash} category="Cash" />
      <AccountCategory accounts={creditCards} category="Credit Cards" />
      <AccountCategory accounts={loans} category="Loans" />
      <AccountCategory accounts={investments} category="Investments" />
      <AccountCategory accounts={property} category="Property" />
      <NetWorth accounts={accounts} />
    </div>
  )
}
