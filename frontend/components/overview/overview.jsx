import React from 'react'
import AccountIndexContainer from '../accounts/accounts_index_container'
import Modal from '../modal'
export default function overview({getAccounts}) {
  // const [accounts, setAccounts] = useState([])
  // setAccounts(getAccounts())


  return (
    <main className="overview-page">
      <Modal />
      <AccountIndexContainer />
      {/* <div className="accounts-filler"></div>
      <div className='chart-filler'></div> */}
    </main>
  )
} 
  

