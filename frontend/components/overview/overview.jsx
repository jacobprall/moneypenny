import React, {useEffect} from 'react'
import AccountIndexContainer from '../accounts/accounts_index_container'
import Modal from '../modal'
export default function overview({getAccounts}) {
  // const [accounts, setAccounts] = useState([])
  // setAccounts(getAccounts())
  useEffect(() => {
    getAccounts()
  }, [])

  return (
    <main className="overview-page">
      <AccountIndexContainer />
      {/* <div className="accounts-filler"></div>
      <div className='chart-filler'></div> */}
    </main>
  )
} 
  

