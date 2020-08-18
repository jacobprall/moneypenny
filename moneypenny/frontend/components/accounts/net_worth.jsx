import React from 'react'

export default function net_worth({accounts}) {
  const assets = accounts.filter((account) => (
    account.debit
  )).map((account) => (
    account.balance
  )).reduce((acc= 0, account) => (
    account + acc
  ));
  const liabilities = accounts.filter((account) => (
    !account.debit
  )).map((account) => (
    account.balance
  )).reduce((acc = 0, account) => (
    account + acc
  ));

  const netWorth = assets - liabilities
  
  
  
  return (
    <div className="net-worth">
      <span className="net-worth-label">Assets</span>
      <span className="net-worth-label">Liabilities</span>
      <span className="net-worth-label">Net Worth</span>

      <div className>
        <span className="net-worth-assets">{assets}</span>
        <span className="net-worth-liabilities">{liabilities}</span>
        <span className="net-worth-data">{netWorth}</span>
      </div>

    </div>
  )
}
